# M6 Tile-Parallel Executor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close the remaining performance gap to libavm by executing tile decode in parallel. At M6 exit, the Rust backend runs within ~10% of libavm on the representative corpus at matching thread counts, conformance still 100%, TSan clean.

**Architecture:** The `TileExecutor` trait already exists (M0 shipped the sequential impl). M6 adds a threaded implementation of the same trait. No state-machine changes — only lifetime adjustments on the ref-frame manager so tiles can read DPB slots concurrently. The threaded executor is selected via `DecoderBuilder::threads()`; the existing `threads` field on the builder, which has been a no-op for the Rust backend since M0, finally does something. No frame-level pipelining (deferred per spec §3 M6 scope).

**Tech Stack:** Rust 2021. Either `rayon` (new dep, simplest) or a small custom thread pool. Decision during Task A based on per-tile overhead measurement.

**Spec reference:** `docs/superpowers/specs/2026-04-11-rustavm-pure-rust-decoder-design.md` §3 M6.

**Prerequisites:** M5 complete (SIMD landed, conformance 100%, perf within ~25% of libavm single-threaded).

---

## Phase A — Thread-pool decision

### Task A.1: Measure per-tile overhead

**Files:**
- Create: `benches/tile_dispatch.rs`

- [ ] **Step 1: Microbenchmark that dispatches 1024 tiny `|i| { black_box(i) }` closures through both a rayon `ThreadPool::install` + `for_each` and a bespoke `crossbeam::scope` + channel approach**
- [ ] **Step 2: Measure per-closure overhead**
- [ ] **Step 3: Compare with the expected per-tile work size** (from M5 profiles — how long does it take scalar + SIMD to decode one tile on average)
- [ ] **Step 4: If rayon overhead is <5% of per-tile decode time, pick rayon. Otherwise build the custom pool.**
- [ ] **Step 5: Document the decision in `docs/superpowers/plans/m6-thread-pool-decision.md` with the measured numbers**
- [ ] **Step 6: Commit** — `rustavm: measure tile-dispatch overhead for M6 pool choice`

### Task A.2: Add the chosen dependency (if rayon)

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add `rayon = "1"` as a non-default feature** — `threaded-tiles` feature flag so users who want single-threaded builds can opt out
- [ ] **Step 2: Commit** — `rustavm: add rayon under threaded-tiles feature`

---

## Phase B — DPB lifetime refactor

### Task B.1: Make ref frames `Arc<FrameBuffer<P>>` end-to-end

**Files:**
- Modify: `src/decoder/inter/refs.rs`
- Modify: `src/decoder/core.rs`

In M2 `ReferenceFrame.frame` was already `Arc<FrameBuffer<u8>>`, anticipating this. M6 confirms the shape and removes any remaining borrows that would prevent concurrent readers.

- [ ] **Step 1: Audit every `&FrameBuffer` / `&PlaneBuffer` reachable from inside tile decode** — they must be derived from an `Arc` clone, not from a mutable borrow on the current frame
- [ ] **Step 2: The currently-decoding frame is the only mutable one — wrap it in per-tile slices so each tile gets a `&mut` to its own sub-rectangle**
- [ ] **Step 3: Tests that exercise concurrent read of a ref frame** — two tiles each calling `predict_inter_single_ref` against the same reference
- [ ] **Step 4: Commit** — `rustavm: DPB ref frames read-safe across threads`

### Task B.2: Split the current frame into per-tile mutable slices

**Files:**
- Modify: `src/decoder/frame_buffer.rs`
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: `FrameBuffer::split_into_tiles(&mut self, tile_layout) -> Vec<TileMut<'_, P>>`** — returns non-overlapping `&mut` slices, one per tile rectangle

Each `TileMut` carries `&mut [P]` for each plane plus stride info. Lifetimes tie it to the parent `FrameBuffer`. Non-overlap is enforced by construction.

- [ ] **Step 2: Unit tests** — split, write from multiple threads via `std::thread::scope`, verify writes land in the right places without overlap
- [ ] **Step 3: Commit** — `rustavm: FrameBuffer::split_into_tiles for parallel reconstruction`

---

## Phase C — Threaded TileExecutor

### Task C.1: Implement `ThreadedTileExecutor`

**Files:**
- Modify: `src/decoder/threaded.rs` (expand from M0 stub)
- Modify: `src/decoder/executor.rs`

- [ ] **Step 1: Struct**

```rust
//! Threaded tile executor.
#![forbid(unsafe_code)]

#[cfg(feature = "threaded-tiles")]
pub(crate) struct ThreadedTileExecutor {
    pool: rayon::ThreadPool,
}

#[cfg(feature = "threaded-tiles")]
impl ThreadedTileExecutor {
    pub fn new(num_threads: usize) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .expect("thread pool");
        Self { pool }
    }
}

#[cfg(feature = "threaded-tiles")]
impl super::executor::TileExecutor for ThreadedTileExecutor {
    fn for_each_tile<F>(&self, num_tiles: usize, f: F)
    where F: Fn(usize) + Sync,
    {
        self.pool.install(|| {
            use rayon::prelude::*;
            (0..num_tiles).into_par_iter().for_each(|i| f(i));
        });
    }
}
```

- [ ] **Step 2: Unit test** — executor visits every tile, regardless of order
- [ ] **Step 3: Commit** — `rustavm: ThreadedTileExecutor with rayon`

### Task C.2: Select executor from `DecoderBuilder::threads()`

**Files:**
- Modify: `src/decoder/mod.rs`
- Modify: `src/backend/rust.rs`

- [ ] **Step 1: When `threads > 1` and `threaded-tiles` feature is enabled, use `ThreadedTileExecutor::new(threads)`**
- [ ] **Step 2: Otherwise fall back to `Sequential`**
- [ ] **Step 3: Integration test** — decode a multi-tile fixture through both `threads=1` and `threads=4`, assert byte-identical output
- [ ] **Step 4: Commit** — `rustavm: wire DecoderBuilder::threads into tile executor`

### Task C.3: Per-tile state isolation

**Files:**
- Modify: `src/decoder/core.rs`

Each parallel tile needs its own `BacReader` (already the case — tiles have separate entropy-coded payloads), its own `TileContext` (CDFs + block-info grid scratch), and its own `TileMut` slice into the frame buffer. All of these are owned by the closure passed to `for_each_tile`.

- [ ] **Step 1: `decode_tile` refactored to take a `TileInput` struct (bitstream slice, sh, fh, ref_frames, tile_mut, tile_ctx) and produce local state only**
- [ ] **Step 2: Verify no shared mutable state between tiles** — audit for any `&mut` reaching across the closure boundary
- [ ] **Step 3: Commit** — `rustavm: isolate per-tile state for parallel decode`

---

## Phase D — Correctness checks

### Task D.1: Conformance under both executors

- [ ] **Step 1: `cargo test -p rustavm --test conformance_test -- --test-threads=1` — single-threaded harness, Sequential executor**
- [ ] **Step 2: `cargo test -p rustavm --test conformance_test --features threaded-tiles -- --test-threads=1` — single-threaded harness, Threaded executor** (different thread counts: 1, 2, 4, 8)
- [ ] **Step 3: Both must pass 100%**
- [ ] **Step 4: Commit log shows these as passing runs before declaring M6 done**

### Task D.2: TSan run

- [ ] **Step 1: `RUSTFLAGS="-Zsanitizer=thread" cargo +nightly test -p rustavm --features threaded-tiles --target x86_64-unknown-linux-gnu`**
- [ ] **Step 2: Fix any reported races**
- [ ] **Step 3: Add the TSan run to nightly CI**
- [ ] **Step 4: Commit** — `rustavm: TSan clean under threaded executor`

---

## Phase E — Perf gate

### Task E.1: Run the benchmark under thread counts

**Files:**
- Modify: `benches/decoder.rs`

- [ ] **Step 1: Extend the decoder bench to sweep `threads={1, 2, 4, 8}` for both libavm and the Rust backend**
- [ ] **Step 2: Report ratio at matching thread counts**
- [ ] **Step 3: Exit criterion: Rust/libavm ratio ≤ 1.10 on the representative corpus at thread count = physical cores**
- [ ] **Step 4: If not met, diagnose with profiling before moving on** — common causes: lock contention in rayon work stealing, false sharing on the block-info grid, MC kernel cache misses on cross-tile refs

### Task E.2: M6 exit checklist

- [ ] 100% conformance under Sequential executor
- [ ] 100% conformance under Threaded executor at 1, 2, 4, 8 threads
- [ ] TSan clean
- [ ] Miri passes (threading modules are safe-only so Miri can still run them)
- [ ] `cargo clippy -p rustavm -- -D warnings` clean
- [ ] Rust/libavm perf ratio ≤ 1.10 on the representative corpus at physical-core thread count
- [ ] No performance cliff at `threads=1` — the threaded executor at `threads=1` must not be slower than Sequential

Merge and start on M7.
