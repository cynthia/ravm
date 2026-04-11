# M5 SIMD Kernels Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close most of the performance gap to libavm by landing SIMD implementations of the hot kernels behind the existing `Kernels` trait. Priority order is driven by profiling. Target at M5 exit: within ~25% of libavm single-threaded on the representative corpus. The remaining gap closes in M6 via threading.

**Architecture:** All SIMD work lives under `src/decoder/kernels/simd_x86.rs` (AVX2 primary, AVX-512 where profiling says it helps) and `src/decoder/kernels/simd_aarch64.rs` (NEON). The `Kernels` trait already exists — SIMD impls are drop-in replacements for the scalar impl. Runtime CPU-feature detection in `Kernels::detect()` picks the best available implementation at startup. `unsafe` is permitted inside `kernels/simd_*` per spec §4.3; the `#[forbid(unsafe_code)]` in other modules guarantees the SIMD surface stays bounded.

**Tech Stack:** Rust 2021 stable; `core::arch::x86_64::*` for x86, `core::arch::aarch64::*` for ARM. `std::simd` where it produces clean codegen but prefer intrinsics for the hottest kernels. New dev-dependency on `criterion` for benchmarks (if not already present).

**Spec reference:** `docs/superpowers/specs/2026-04-11-rustavm-pure-rust-decoder-design.md` §3 M5.

**Prerequisites:** M4 complete (100% Main-profile conformance at 8-bit + 10-bit, scalar, single-threaded).

---

## Phase A — Measurement infrastructure

### Task A.1: Benchmark harness

**Files:**
- Create: `benches/decoder.rs`
- Modify: `Cargo.toml` — add `criterion` dev-dependency if not present

- [ ] **Step 1: Criterion benches decoding each fixture from the representative corpus** — see spec open question 3; pin the corpus as part of this task
- [ ] **Step 2: Two benches per fixture: one runs the Rust backend, one runs libavm**
- [ ] **Step 3: Report relative perf (Rust time / libavm time) as the headline metric**
- [ ] **Step 4: Commit** — `rustavm: criterion benches for decoder perf`

### Task A.2: Profile the scalar decoder

- [ ] **Step 1: Run under `perf record` / `samply` on the representative corpus**
- [ ] **Step 2: Capture the top 20 hotspots**
- [ ] **Step 3: Document the hot set in `docs/superpowers/plans/m5-hot-kernels.md`**

Expected hot set (from AV1/AV2 experience — the actual numbers drive priority):
1. Inverse transforms (especially 8×8 and 16×16 DCT_DCT)
2. Subpel MC (translation)
3. CDEF filter
4. Deblocking (vertical edges)
5. Wiener LR
6. Intra directional predictors
7. Self-guided LR
8. Warped MC

**Commit the hot-kernel document but do not commit anything about SIMD implementation yet** — that's the rest of M5.

---

## Phase B — SIMD backend scaffolding

### Task B.1: Runtime dispatch

**Files:**
- Modify: `src/decoder/kernels/mod.rs`
- Modify: `src/decoder/kernels/simd_x86.rs` (expand from M0 stub)
- Modify: `src/decoder/kernels/simd_aarch64.rs` (expand from M0 stub)

- [ ] **Step 1: CPU feature detection**

```rust
pub(crate) fn detect() -> &'static dyn Kernels {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return &simd_x86::Avx2;
        }
    }
    #[cfg(target_arch = "aarch64")]
    {
        if std::arch::is_aarch64_feature_detected!("neon") {
            return &simd_aarch64::Neon;
        }
    }
    &scalar::Scalar
}
```

- [ ] **Step 2: `Avx2` struct that defaults every kernel method to the scalar impl** — SIMD overrides land family-by-family in later tasks
- [ ] **Step 3: Same for `Neon`**
- [ ] **Step 4: Test that `detect()` returns AVX2 on capable hardware, falls back otherwise**
- [ ] **Step 5: Commit** — `rustavm: SIMD backend scaffolding with runtime dispatch`

### Task B.2: Forbid pitfalls

**Files:**
- Modify: `src/decoder/kernels/simd_x86.rs`
- Modify: `src/decoder/kernels/simd_aarch64.rs`

- [ ] **Step 1: Add `#![deny(unsafe_op_in_unsafe_fn)]`**
- [ ] **Step 2: Document the safety convention** — every `#[target_feature]` function is `unsafe fn` and only callable from a dispatch site that checked `is_x86_feature_detected!`
- [ ] **Step 3: Commit** — `rustavm: SIMD module safety conventions`

---

## Phase C — Hot-kernel SIMD rollout

For each family in the profile-ordered priority list, the same pattern applies. Per-family tasks are structured as follows:

```
For each <Kernel Family>:

Task C.X.1 — AVX2 impl
  Step 1: Write the AVX2 intrinsics function in simd_x86.rs
  Step 2: Override the Kernels::<method> impl on Avx2
  Step 3: Run the entire conformance suite (regression gate — no new bugs allowed)
  Step 4: Run the family's benchmark, record speedup
  Step 5: Commit

Task C.X.2 — NEON impl
  Step 1: Same in simd_aarch64.rs
  Step 2–5: as above
```

**Do not try to land all kernels in one commit. One kernel family per commit, regression-gated by the conformance suite.**

The priority list below is the starting order — reshuffle based on the profile from Task A.2.

### Task C.1: Inverse transforms — `inv_dct_NxN`, `inv_adst_NxN`

Highest impact. Start with the square sizes, then rectangular.

- [ ] **AVX2 4×4, 8×8, 16×16 DCT_DCT** — one commit
- [ ] **AVX2 ADST / FLIPADST variants** — one commit
- [ ] **AVX2 32×32 / 64×64** — one commit
- [ ] **AVX2 rectangular sizes** — one commit
- [ ] **NEON mirror** — one commit per size group

### Task C.2: Subpel MC

- [ ] **AVX2 8-tap horizontal pass — luma**
- [ ] **AVX2 8-tap vertical pass — luma**
- [ ] **AVX2 combined H+V — luma**
- [ ] **AVX2 chroma MC**
- [ ] **NEON mirror**

### Task C.3: CDEF filter

- [ ] **AVX2 `cdef_filter_block`** (the inner filter loop; direction search stays scalar unless profile says otherwise)
- [ ] **NEON `cdef_filter_block`**

### Task C.4: Deblocking

- [ ] **AVX2 4-tap and 6-tap luma filters**
- [ ] **AVX2 8-tap and 14-tap luma filters**
- [ ] **AVX2 chroma filters**
- [ ] **NEON mirror**

### Task C.5: Loop restoration

- [ ] **AVX2 Wiener (separable 7-tap)**
- [ ] **AVX2 self-guided (box filter + combination)**
- [ ] **NEON mirror**

### Task C.6: Intra directional predictors

- [ ] **AVX2 `predict_directional` — the common kernel**
- [ ] **AVX2 edge filter + upsample**
- [ ] **NEON mirror**

### Task C.7: Warped MC

- [ ] **AVX2 `warped_mc` — 8-tap with affine phase derivation**
- [ ] **NEON mirror**

### Task C.8: Smooth / Paeth / CFL (lower priority, based on profile)

- [ ] **AVX2 impls if profiling shows they matter**

---

## Phase D — Re-profile and iterate

### Task D.1: Post-rollout profile pass

- [ ] **Step 1: Re-run the profiler against the SIMD build**
- [ ] **Step 2: Identify anything still in the top 10 that hasn't been SIMDed**
- [ ] **Step 3: Add one follow-up task per kernel worth optimizing, land them with the same commit discipline as Phase C**

### Task D.2: Cross-kernel tuning

- [ ] **Step 1: Look for opportunities to fuse kernels** — e.g. MC + residual add in one pass if it reduces memory traffic
- [ ] **Step 2: Only land fusions that show a measurable win on the representative corpus**

---

## Phase E — M5 exit gate

### Task E.1: Full conformance + perf check

- [ ] **Step 1: `cargo test -p rustavm --test conformance_test -- --nocapture` — still 100%**

The regression discipline in Phase C should have prevented conformance regressions already, but the exit gate re-runs the whole suite once more as proof.

- [ ] **Step 2: `cargo bench -p rustavm` — capture the final perf delta**
- [ ] **Step 3: If the ratio is ≤ 1.25× libavm on the representative corpus, M5 exits**
- [ ] **Step 4: If the ratio is worse, file follow-up kernels against the profile and re-iterate before declaring M5 done**

### Task E.2: M5 exit checklist

- [ ] 100% Main-profile conformance still passes
- [ ] M0–M4 test suites still pass
- [ ] Rust backend runs at ≤ 1.25× libavm single-threaded wall-clock on the representative corpus
- [ ] Every `unsafe` block in `kernels/simd_*` has a `// SAFETY:` comment
- [ ] `cargo clippy -p rustavm -- -D warnings -W clippy::undocumented_unsafe_blocks` clean
- [ ] Miri passes on every module outside `kernels/simd_*`
- [ ] No performance cliff: no single kernel should be slower under SIMD than under scalar (if it is, revert that family)

Merge and start on M6.
