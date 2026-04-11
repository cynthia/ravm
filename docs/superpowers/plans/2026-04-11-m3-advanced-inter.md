# M3 Advanced Inter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add every remaining inter coding tool: compound prediction (AVG, DIST, WEDGE, DIFFWTD, interintra), warped motion (per-block + global affine), OBMC, global motion beyond translation, and the full ref_mv context. By the end of M3 the Rust backend passes all inter conformance vectors (single-ref, compound, warped, global).

**Architecture:** All work lives under `src/decoder/inter/`. Compound prediction adds a second prediction path that blends two reference-frame predictions through a per-pixel mask. Warped motion adds a different MC kernel — 8-tap affine rather than 8-tap translation. OBMC is a blend pass over already-predicted blocks using neighbor MVs. No new top-level modules.

**Tech Stack:** Rust 2021. No new external crates.

**Spec reference:** `docs/superpowers/specs/2026-04-11-rustavm-pure-rust-decoder-design.md` §3 M3.

**Prerequisites:** M2 complete (all single-ref inter conformance vectors passing).

---

## Phase A — Compound MV derivation

### Task A.1: Compound ref frame selection

**Files:**
- Modify: `src/decoder/inter/refs.rs`
- Modify: `src/decoder/inter/mv.rs`

- [ ] **Step 1: Extend `read_ref_frame` to decode compound refs** — spec §5.11.25 (unidirectional compound, bidirectional compound)
- [ ] **Step 2: Extend `find_mv_stack` to return compound MV candidates** — each stack entry carries two MVs (one per ref)
- [ ] **Step 3: Full spec §7.10.2 — ref_mv_idx for compound modes, NEW_NEW / NEAREST_NEAREST / NEAR_NEAR / NEAR_NEW / NEW_NEAR / NEW_NEAREST / NEAREST_NEW / GLOBAL_GLOBAL mode decoding**
- [ ] **Step 4: Tests** — MV stack against libavm on compound vectors
- [ ] **Step 5: Commit** — `rustavm: compound MV stack and ref_mv_idx selection`

### Task A.2: Full ref_mv context

**Files:**
- Modify: `src/decoder/inter/mv.rs`

- [ ] **Step 1: `mode_context` derivation** — spec §7.10.5, the 8-bit context packing weight, new_mv, global_mv, ref_mv context bits
- [ ] **Step 2: Wire into the appropriate CDF selection for mode read**
- [ ] **Step 3: KATs against libavm on complex neighbor configurations**
- [ ] **Step 4: Commit** — `rustavm: complete ref_mv mode context derivation`

---

## Phase B — Compound prediction blending

### Task B.1: AVG and DIST compound modes

**Files:**
- Modify: `src/decoder/inter/mc.rs`
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: `predict_inter_compound_avg`** — average of two predictors
- [ ] **Step 2: `predict_inter_compound_dist`** — distance-weighted blend per spec §7.11.3.7 (requires order-hint-based weight selection)
- [ ] **Step 3: Add kernel trait methods for scalar blends**
- [ ] **Step 4: KATs**
- [ ] **Step 5: Commit** — `rustavm: AVG and DIST compound prediction`

### Task B.2: WEDGE compound

**Files:**
- Modify: `src/decoder/inter/mc.rs`

- [ ] **Step 1: Port wedge codebook tables from spec §9.2 (16 wedges per block size, multiple block sizes)**
- [ ] **Step 2: `wedge_mask(bsize, wedge_index, flip)` -> &'static [u8] lookup**
- [ ] **Step 3: Per-pixel mask blend kernel**
- [ ] **Step 4: `read_wedge_params` in inter block decode**
- [ ] **Step 5: KATs**
- [ ] **Step 6: Commit** — `rustavm: WEDGE compound prediction with codebook`

### Task B.3: DIFFWTD compound

**Files:**
- Modify: `src/decoder/inter/mc.rs`

- [ ] **Step 1: `compute_diffwtd_mask(p0, p1, mode)` — per-pixel difference-based weight computation per spec §7.11.3.9**
- [ ] **Step 2: Apply mask to blend p0/p1**
- [ ] **Step 3: KATs**
- [ ] **Step 4: Commit** — `rustavm: DIFFWTD compound prediction`

### Task B.4: Interintra

**Files:**
- Modify: `src/decoder/inter/mc.rs`

- [ ] **Step 1: `predict_interintra(block)`** — compute the inter prediction as normal, then blend with an intra prediction (only a few modes allowed: DC, V, H, SMOOTH) per spec §7.11.3.10
- [ ] **Step 2: Interintra wedge support (optional sub-mode)**
- [ ] **Step 3: KATs**
- [ ] **Step 4: Commit** — `rustavm: interintra compound mode`

---

## Phase C — Warped and global motion

### Task C.1: Full global motion param parsing

**Files:**
- Modify: `src/bitstream.rs`

- [ ] **Step 1: Port `global_motion_params` fully — IDENTITY, TRANSLATION, ROTZOOM, AFFINE types — spec §5.9.24**
- [ ] **Step 2: Parameter shearing and matrix validation per spec §7.9.1**
- [ ] **Step 3: Tests**
- [ ] **Step 4: Commit** — `rustavm: full global motion parameter parsing`

### Task C.2: Affine warped MC kernel

**Files:**
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: `warped_filter` table — spec §9.7 6-tap filter coefficients at 1/64-pel phase resolution**
- [ ] **Step 2: `Kernels::warped_mc(src, src_stride, dst, dst_stride, w, h, warp_params)` with horizontal + vertical 8-tap passes using the warp matrix**
- [ ] **Step 3: KATs per warp-params tuple against libavm**
- [ ] **Step 4: Commit** — `rustavm: scalar warped MC kernel`

### Task C.3: Per-block warped motion

**Files:**
- Modify: `src/decoder/inter/mc.rs`

- [ ] **Step 1: Warp parameter derivation from neighboring MVs (spec §7.11.4.1) — samples neighbor MVs, least-squares fit to an affine matrix**
- [ ] **Step 2: `predict_inter_warped(block, warp_params, ref_frame)` dispatch**
- [ ] **Step 3: Warped motion mode read (`motion_mode`) per spec §5.11.28**
- [ ] **Step 4: Tests**
- [ ] **Step 5: Commit** — `rustavm: per-block warped motion`

### Task C.4: Global motion integration

**Files:**
- Modify: `src/decoder/inter/mc.rs`

- [ ] **Step 1: When mode == GLOBAL or GLOBAL_GLOBAL and the frame's global motion params are non-translation, dispatch through `warped_mc` with the frame-level matrix**
- [ ] **Step 2: Tests on vectors with non-translation global motion**
- [ ] **Step 3: Commit** — `rustavm: non-translation global motion uses warped MC`

---

## Phase D — OBMC

### Task D.1: OBMC blend implementation

**Files:**
- Modify: `src/decoder/inter/mc.rs`
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: OBMC mask tables per spec §9.8 — per-direction blend weights, one set per block size**
- [ ] **Step 2: `obmc_blend_above` / `obmc_blend_left` kernels — apply neighbor prediction with falloff mask**
- [ ] **Step 3: `predict_inter_obmc(block)` pipeline — runs normal MC first, then for each neighbor above/left that has a different MV, recomputes prediction using neighbor's MV, blends with mask**
- [ ] **Step 4: KATs**
- [ ] **Step 5: Commit** — `rustavm: OBMC overlap blending`

### Task D.2: `motion_mode` selection

**Files:**
- Modify: `src/decoder/inter/mv.rs`
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: Read `motion_mode` (SIMPLE / OBMC / WARPED) when `is_motion_mode_switchable=1`**
- [ ] **Step 2: Dispatch to the appropriate prediction path**
- [ ] **Step 3: Tests**
- [ ] **Step 4: Commit** — `rustavm: motion_mode switch between SIMPLE/OBMC/WARPED`

---

## Phase E — Skip mode

### Task E.1: Skip_mode path

**Files:**
- Modify: `src/decoder/core.rs`

Skip mode is a shortcut encoding: when `skip_mode=1` on a block, the encoder signals only a skip flag and the decoder infers a specific compound ref pair + mode + MVs from frame-level state.

- [ ] **Step 1: Frame-level skip_mode ref-pair derivation (spec §7.9.2)**
- [ ] **Step 2: Per-block `skip_mode` flag read**
- [ ] **Step 3: When skip_mode=1, forcefully set mode=NEAREST_NEAREST with the inferred ref pair**
- [ ] **Step 4: Tests**
- [ ] **Step 5: Commit** — `rustavm: skip_mode inferred compound prediction`

---

## Phase F — Full inter conformance gate

### Task F.1: Run compound + warped + OBMC conformance subsets

- [ ] **Step 1: Populate cache for every remaining inter tag**

```bash
bash tests/conformance/fetch.sh compound_inter warped_inter obmc_inter global_inter
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rustavm --test conformance_test -- --nocapture
```

All inter-family tags should pass — single-ref (from M2), compound, warped, OBMC, and global.

- [ ] **Step 3: Diff-driven debugging for any failures** — per spec R2, expect MV-derivation bugs here to be the dominant failure mode
- [ ] **Step 4: Commit fixes one root-cause at a time**

### Task F.2: M3 exit checklist

- [ ] All inter conformance vectors pass (single-ref + compound + warped + OBMC + global)
- [ ] M0/M1/M2 test suites still pass
- [ ] `cargo miri test -p rustavm` passes on scalar core
- [ ] `cargo clippy -p rustavm -- -D warnings` clean
- [ ] `src/decoder/inter/` has no `TODO`, `FIXME`, or `unimplemented!()`

Merge and start on M4.
