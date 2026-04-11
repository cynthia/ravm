# M4 Post-Filters and 10-bit Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship every remaining correctness feature — deblocking, CDEF, loop restoration, film grain synthesis — and complete the 10-bit retrofit across every kernel. By the end of M4, the Rust backend passes **100% of Main-profile conformance vectors at the scalar, single-threaded tier**. This is the spec's named correctness finish line; M5–M7 add no decode capability.

**Architecture:** Post-filters are added in pipeline order per spec §7.13–7.16: deblocking → CDEF → LR → film grain. Each gets its own outer module (M0 scaffolds) plus kernel methods on `Kernels`. 10-bit support is a generic expansion of every kernel and buffer that currently monomorphizes to `u8`; because M0 built in the `Pixel` trait from day one, this is monomorphization work rather than a rewrite (per spec risk R4). Chroma format expansion (4:2:2, 4:4:4) is done if conformance vectors require it.

**Tech Stack:** Rust 2021. No new external crates.

**Spec reference:** `docs/superpowers/specs/2026-04-11-rustavm-pure-rust-decoder-design.md` §3 M4.

**Prerequisites:** M3 complete (all inter conformance vectors passing at 8-bit 4:2:0).

---

## Phase A — Deblocking filter

### Task A.1: Edge identification and strength derivation

**Files:**
- Modify: `src/decoder/loopfilter.rs`

- [ ] **Step 1: Types** — `Edge { row, col, vertical, plane, length }`, `EdgeStrength { filter_level, limit, blimit, thresh }`
- [ ] **Step 2: Frame-level `LoopFilterParams` consumption** (parsed in M1, used here)
- [ ] **Step 3: `compute_edge_strengths(block_info_grid)`** per spec §7.14.3 — walks every TU/PU edge, computes filter_level from block params + seg features + delta_lf
- [ ] **Step 4: Unit tests — edge-strength computation on known block configurations**
- [ ] **Step 5: Commit** — `rustavm: deblock edge identification and strength`

### Task A.2: Filter kernels

**Files:**
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: `filter4_luma`, `filter6_luma`, `filter8_luma`, `filter14_luma` — 4/6/8/14-tap edge filters from spec §7.14.4**
- [ ] **Step 2: `filter4_chroma`, `filter6_chroma` — chroma edges**
- [ ] **Step 3: Normative masking** — decision functions hev/mask/flat per spec §7.14.4.2
- [ ] **Step 4: KATs against libavm on hand-picked edge configurations**
- [ ] **Step 5: Commit** — `rustavm: scalar deblock filter kernels`

### Task A.3: Frame-level deblock pass

**Files:**
- Modify: `src/decoder/loopfilter.rs`
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: `apply_deblock(fb, block_info_grid, params)` driver**
- [ ] **Step 2: Apply vertical edges first, then horizontal, per spec**
- [ ] **Step 3: Wire into `core.rs` after all tiles are reconstructed**
- [ ] **Step 4: Conformance vector spot check on a vector that exercises deblock**
- [ ] **Step 5: Commit** — `rustavm: deblock pass integrated into frame pipeline`

---

## Phase B — CDEF

### Task B.1: CDEF primary/secondary directional filter kernel

**Files:**
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: `cdef_direction(block)` — direction search per spec §7.15.2**
- [ ] **Step 2: `cdef_filter_block(dst, src, primary_strength, secondary_strength, direction, damping)` — per spec §7.15.3**
- [ ] **Step 3: KATs — CDEF output on fixed inputs matches libavm**
- [ ] **Step 4: Commit** — `rustavm: scalar CDEF direction + filter kernel`

### Task B.2: Frame-level CDEF pass

**Files:**
- Modify: `src/decoder/cdef.rs`
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: `CdefParams` consumption (parsed in M1)**
- [ ] **Step 2: Per-SB strength index read — spec §5.11.57**
- [ ] **Step 3: Per-SB filter dispatch**
- [ ] **Step 4: Edge handling (frame boundary, skip blocks) per spec §7.15.1**
- [ ] **Step 5: Wire into `core.rs` after deblock**
- [ ] **Step 6: Commit** — `rustavm: CDEF pass integrated after deblock`

---

## Phase C — Loop restoration

### Task C.1: Wiener filter

**Files:**
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: `wiener_filter(src, dst, filter_coeffs_h, filter_coeffs_v)` per spec §7.16.3 — separable 7-tap horizontal + 7-tap vertical**
- [ ] **Step 2: Coefficient reconstruction — spec §5.11.58 decodes only part of each coefficient; remaining symmetry-implied values filled in**
- [ ] **Step 3: KATs against libavm**
- [ ] **Step 4: Commit** — `rustavm: Wiener loop restoration kernel`

### Task C.2: Self-guided restoration

**Files:**
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: Two-pass box filter at radii r1, r2 per spec §7.16.4**
- [ ] **Step 2: Per-pixel weighted combination with xq0, xq1 parameters**
- [ ] **Step 3: KATs**
- [ ] **Step 4: Commit** — `rustavm: self-guided loop restoration kernel`

### Task C.3: Frame-level LR pass

**Files:**
- Modify: `src/decoder/restoration.rs`
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: `LoopRestorationParams` consumption (parsed in M1)**
- [ ] **Step 2: Per-plane restoration unit size determination**
- [ ] **Step 3: Per-unit type selection (NONE / WIENER / SGRPROJ / SWITCHABLE)**
- [ ] **Step 4: Per-unit parameter read + kernel dispatch**
- [ ] **Step 5: Wire after CDEF**
- [ ] **Step 6: Commit** — `rustavm: loop restoration pass integrated`

---

## Phase D — Film grain synthesis

### Task D.1: AR grain generation

**Files:**
- Modify: `src/decoder/film_grain.rs`
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: Port the AR(1) / AR(2) / AR(3) grain-generation recurrence per spec §7.18.3.2**
- [ ] **Step 2: Per-frame grain seed state tracking**
- [ ] **Step 3: Scaling LUT application per luma/chroma value**
- [ ] **Step 4: Apply grain to output image (not to reference frames)**
- [ ] **Step 5: KATs against libavm's film grain output**
- [ ] **Step 6: Commit** — `rustavm: film grain synthesis per AV2 §7.18`

### Task D.2: Wire into output path

**Files:**
- Modify: `src/decoder/core.rs`
- Modify: `src/backend/rust.rs`

- [ ] **Step 1: Film grain happens after LR but only on the *display* output, not what's stored into the DPB**
- [ ] **Step 2: `output_image = film_grain.apply(&reconstructed)`**
- [ ] **Step 3: Integration test on a vector with film_grain_params_present=1**
- [ ] **Step 4: Commit** — `rustavm: apply film grain to output image only`

---

## Phase E — 10-bit retrofit

### Task E.1: Add `u16` Pixel impl

**Files:**
- Modify: `src/decoder/frame_buffer.rs`

- [ ] **Step 1: Add `Pixel for u16 { const BIT_DEPTH: u32 = 10; const MAX: u32 = 1023; }`**

Note: BIT_DEPTH is runtime-selected between 10 and 12 in real streams. We track it at runtime via `FrameBuffer::bit_depth` in addition to the compile-time `Pixel` trait, because some kernels need the actual bit depth (for rounding shifts and clip values). Keep `Pixel` as `u16` and add a `bit_depth: u8` field on `FrameBuffer` / `PlaneBuffer` for the actual value.

- [ ] **Step 2: Unit tests for `u16` buffers**
- [ ] **Step 3: Commit** — `rustavm: u16 Pixel impl for 10-bit support`

### Task E.2: Monomorphize every kernel over Pixel

**Files:**
- Modify: `src/decoder/kernels/mod.rs`
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: Change `Kernels` trait methods to be generic over `P: Pixel`** — e.g. `fn inv_dct<P: Pixel>(&self, ...)`

This is a semver-invisible but large edit. Strategy: change one kernel family at a time, one commit each. After each change, re-run the full conformance suite (still 8-bit only) — no regressions permitted.

- [ ] **Step 2: Kernel families in order** (one commit each):
  1. Inverse transforms
  2. Subpel MC (translation)
  3. Warped MC
  4. Intra predictors
  5. Deblock filters
  6. CDEF kernel
  7. Wiener and self-guided LR
  8. Film grain application

- [ ] **Step 3: Post-retrofit conformance check** — rerun everything at 8-bit, confirm no regressions

### Task E.3: Wire 10-bit in the frame driver

**Files:**
- Modify: `src/decoder/core.rs`
- Modify: `src/backend/rust.rs`

- [ ] **Step 1: Dispatch to `FrameBuffer<u16>` when `sh.bit_depth > 8`**
- [ ] **Step 2: Update `avm_image_t` output interop to emit 16-bit planes for 10-bit streams**
- [ ] **Step 3: Integration test on a 10-bit conformance vector**
- [ ] **Step 4: Commit** — `rustavm: 10-bit frame dispatch`

### Task E.4: 10-bit conformance

```bash
bash tests/conformance/fetch.sh ten_bit
cargo test -p rustavm --test conformance_test -- ten_bit --nocapture
```

- [ ] **Step 1: Iterate until all 10-bit vectors pass**
- [ ] **Step 2: Commit fixes**

---

## Phase F — Chroma format expansion (conditional)

### Task F.1: 4:2:2 support

**Files:**
- Modify: `src/decoder/frame_buffer.rs`
- Modify: `src/decoder/kernels/scalar.rs` (any chroma-dimension-sensitive paths)

Required only if the conformance suite includes 4:2:2 vectors. Check the manifest first.

- [ ] **Step 1: Validate `Subsampling::Yuv422` plane dimensions are correct**
- [ ] **Step 2: Chroma MV scaling** — chroma MVs in 4:2:2 use a different horizontal scale
- [ ] **Step 3: CFL in 4:2:2**
- [ ] **Step 4: Conformance run on 4:2:2 vectors**
- [ ] **Step 5: Commit** — `rustavm: 4:2:2 chroma format support`

### Task F.2: 4:4:4 support

Same structure as F.1. Skip if no 4:4:4 vectors.

### Task F.3: 12-bit support

Same structure as 10-bit retrofit, swapping in `P::BIT_DEPTH = 12` handling. Skip if no 12-bit vectors.

---

## Phase G — Full Main-profile conformance gate

### Task G.1: Run the whole conformance suite

- [ ] **Step 1: Populate the cache for every tag**

```bash
bash tests/conformance/fetch.sh --all
```

- [ ] **Step 2: Run everything**

```bash
cargo test -p rustavm --test conformance_test -- --nocapture
```

- [ ] **Step 3: Iterate until green**

### Task G.2: M4 exit checklist

- [ ] **100% of Main-profile conformance vectors pass** — this is the correctness finish line
- [ ] `cargo test -p rustavm` passes clean
- [ ] `cargo miri test -p rustavm` passes on scalar core
- [ ] `cargo clippy -p rustavm -- -D warnings` clean
- [ ] All kernels generic over `Pixel` (the 10-bit retrofit did not leave `u8`-only stragglers)
- [ ] Post-filter pipeline runs in spec order: deblock → CDEF → LR → film grain
- [ ] Film grain is applied only to output, not to reference frames in the DPB
- [ ] `src/decoder/` has no `TODO`, `FIXME`, or `unimplemented!()`

Merge and start on M5. **The rest of the project (M5–M7) is performance and cleanup — no new decode features.**
