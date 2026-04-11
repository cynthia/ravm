# M2 Single-Reference Inter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add single-reference inter-frame decoding. By the end of M2 the Rust backend passes every single-ref P-frame conformance vector. Compound prediction, OBMC, and warped motion are explicitly out of scope (they're M3).

**Architecture:** The new work lives under `src/decoder/inter/`, which was scaffolded but unused in M0 and M1. A reference frame manager (DPB) replaces the KF-only assumption in `core.rs`. MV derivation, subpel MC filters, and ref-list construction land as new modules. The frame driver branches on `frame_type`: KF path is unchanged from M1; INTER path routes through the new inter code.

**Tech Stack:** Rust 2021. No new external crates. Reference frames stored as `Arc<FrameBuffer<u8>>` even in single-threaded execution so M6 inherits the right ownership.

**Spec reference:** `docs/superpowers/specs/2026-04-11-rustavm-pure-rust-decoder-design.md` §3 M2.

**Prerequisites:** M1 complete (all KF-only conformance vectors passing).

---

## Pre-flight

- [ ] **Task 0.1: Expand the conformance manifest**

**Files:**
- Modify: `tests/conformance/manifest.toml`

Add a `single_ref_inter` feature tag to every vector in the corpus that tests P-frames with one reference. Exclude vectors that use compound prediction, OBMC, warped, or global motion — those stay under the `compound_inter` tag for M3.

Commit.

---

## Phase A — Reference frame manager (DPB)

### Task A.1: `ReferenceFrame` and `Dpb`

**Files:**
- Create: `src/decoder/inter/refs.rs`
- Modify: `src/decoder/inter/mod.rs`

- [ ] **Step 1: Define the types**

```rust
//! Reference frame management (DPB).
#![forbid(unsafe_code)]

use std::sync::Arc;
use crate::decoder::frame_buffer::FrameBuffer;

pub(crate) const NUM_REF_FRAMES: usize = 8;

#[derive(Clone)]
pub(crate) struct ReferenceFrame {
    pub frame: Arc<FrameBuffer<u8>>,
    pub order_hint: u8,
    pub width: u32,
    pub height: u32,
    pub frame_type: crate::bitstream::FrameType,
}

pub(crate) struct Dpb {
    slots: [Option<ReferenceFrame>; NUM_REF_FRAMES],
}

impl Dpb {
    pub fn new() -> Self {
        Self { slots: Default::default() }
    }

    /// Store `frame` into the DPB according to `refresh_frame_flags`.
    pub fn refresh(&mut self, refresh_flags: u8, frame: ReferenceFrame) {
        for i in 0..NUM_REF_FRAMES {
            if (refresh_flags >> i) & 1 == 1 {
                self.slots[i] = Some(frame.clone());
            }
        }
    }

    pub fn get(&self, slot: usize) -> Option<&ReferenceFrame> {
        self.slots[slot].as_ref()
    }
}
```

- [ ] **Step 2: Unit tests** — refresh into slot patterns, read back, verify clone semantics under Arc
- [ ] **Step 3: Commit** — `rustavm: add DPB with 8-slot reference storage`

### Task A.2: Order-hint arithmetic

**Files:**
- Modify: `src/decoder/inter/refs.rs`

- [ ] **Step 1: `get_relative_dist(order_hint_bits, a, b)`** per spec §7.9.3
- [ ] **Step 2: Unit tests with known order-hint values**
- [ ] **Step 3: Commit** — `rustavm: order-hint distance arithmetic`

### Task A.3: Dynamic ref-list construction for single ref

**Files:**
- Modify: `src/decoder/inter/refs.rs`

- [ ] **Step 1: `build_ref_list(&self, sh, fh, ref_frame_idx) -> [usize; 7]`** — returns the LAST, LAST2, LAST3, GOLDEN, BWDREF, ALTREF2, ALTREF slot indices per spec §7.9
- [ ] **Step 2: Order-hint sorting**
- [ ] **Step 3: Unit tests on synthetic DPB states**
- [ ] **Step 4: Commit** — `rustavm: build ref list with order-hint sorting`

---

## Phase B — Inter frame header fields

### Task B.1: Parse inter-specific header fields

**Files:**
- Modify: `src/bitstream.rs`

M1 landed the KF path of `parse_uncompressed_frame_header`. M2 adds the INTER branch: `primary_ref_frame`, `refresh_frame_flags`, `ref_frame_idx`, `ref_order_hint`, `allow_high_precision_mv`, `interpolation_filter`, `is_motion_mode_switchable`, `use_ref_frame_mvs`.

- [ ] **Step 1: Extend `UncompressedFrameHeader` with inter fields**
- [ ] **Step 2: Port the INTER branch of spec §5.9.1**
- [ ] **Step 3: Unit tests against a P-frame conformance vector**
- [ ] **Step 4: Commit** — `rustavm: parse inter frame header fields`

### Task B.2: Segmentation carry-over from primary ref

**Files:**
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: When `primary_ref_frame != PRIMARY_REF_NONE`, copy segmentation state from that reference's saved context**
- [ ] **Step 2: Same for CDFs when `disable_cdf_update=0`**
- [ ] **Step 3: `SavedFrameContext` stored on each `ReferenceFrame`**
- [ ] **Step 4: Tests**
- [ ] **Step 5: Commit** — `rustavm: carry saved segmentation/CDF from primary ref`

---

## Phase C — Motion vector derivation

This is the most error-prone inter subsystem per spec risk R2. Expect heavy use of `src/diff.rs` during development.

### Task C.1: `MotionVector` type and basic operations

**Files:**
- Create: `src/decoder/inter/mv.rs`

- [ ] **Step 1: Define the struct**

```rust
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MotionVector {
    pub row: i16,
    pub col: i16,
}

impl MotionVector {
    pub const ZERO: Self = Self { row: 0, col: 0 };
    pub fn is_zero(self) -> bool { self.row == 0 && self.col == 0 }
}
```

- [ ] **Step 2: MV scaling for resampled refs** (spec §7.11.3.3)
- [ ] **Step 3: Unit tests**
- [ ] **Step 4: Commit** — `rustavm: MV type and scaling`

### Task C.2: Spatial MV stack

**Files:**
- Modify: `src/decoder/inter/mv.rs`

- [ ] **Step 1: `find_mv_stack(&BlockInfoGrid, x, y, bsize, ref_frame)`** — walks 8 neighbor positions (4 above + 4 left) gathering candidate MVs, deduplicating, up to 8 candidates
- [ ] **Step 2: Candidate weighting per spec §7.10.2**
- [ ] **Step 3: KAT tests on synthetic neighbor grids**
- [ ] **Step 4: Commit** — `rustavm: spatial MV stack construction`

### Task C.3: Temporal MV stack (TMVP)

**Files:**
- Modify: `src/decoder/inter/mv.rs`

- [ ] **Step 1: Per spec §7.9.2 — scan the reference frame's saved MV grid at positions derived from the current block's coordinates**
- [ ] **Step 2: Projection math for MV scaling across frame distances**
- [ ] **Step 3: Merge with the spatial stack**
- [ ] **Step 4: KATs**
- [ ] **Step 5: Commit** — `rustavm: temporal MV projection and merge`

### Task C.4: MVP selection for single-ref modes

**Files:**
- Modify: `src/decoder/inter/mv.rs`

Single-ref modes are NEAREST, NEAR, NEW, GLOBAL. (NEAR has four sub-variants selecting position 1–4 in the stack.)

- [ ] **Step 1: `select_mvp(mode, stack)` returning the chosen candidate**
- [ ] **Step 2: `read_mv(mode, ctx)` for NEW-mode MV delta decoding** — spec §5.11.31 (mv_joint, mv_class, mv_class0, mv_bits, mv_fr, mv_hp)
- [ ] **Step 3: KATs against libavm-produced MV sequences**
- [ ] **Step 4: Commit** — `rustavm: MVP selection and MV delta read for single-ref modes`

### Task C.5: Global motion (single-ref only, identity/translation subset)

**Files:**
- Modify: `src/decoder/inter/mv.rs`

- [ ] **Step 1: Parse `global_motion_params`** — spec §5.9.24
- [ ] **Step 2: GLOBALMV mode uses the translation component**
- [ ] **Step 3: Rotzoom and affine global models are parsed but used only at M3 compound path**
- [ ] **Step 4: Commit** — `rustavm: GLOBALMV single-ref path`

---

## Phase D — Motion compensation filters

### Task D.1: Subpel filter tables

**Files:**
- Modify: `src/decoder/kernels/scalar.rs`
- Modify: `src/decoder/inter/mc.rs`

AV2 supports four 8-tap filter sets: REGULAR, SMOOTH, SHARP, BILINEAR. Each has 16 phases (1/16-pel precision).

- [ ] **Step 1: Port `SUBPEL_FILTERS[4][16][8]` from spec §7.11.3.4**
- [ ] **Step 2: Hash-test the ported tables**
- [ ] **Step 3: Commit** — `rustavm: port subpel filter tables`

### Task D.2: 8-tap subpel MC kernel

**Files:**
- Modify: `src/decoder/kernels/scalar.rs`
- Modify: `src/decoder/kernels/mod.rs`

- [ ] **Step 1: Add kernel trait method** — `fn subpel_mc(&self, src, src_stride, dst, dst_stride, w, h, mv, filter_y, filter_x)` — with intermediate horizontal pass + vertical pass
- [ ] **Step 2: Scalar impl**
- [ ] **Step 3: KATs per filter set for a few common (w, h, subpel_x, subpel_y) tuples vs libavm**
- [ ] **Step 4: Commit** — `rustavm: scalar 8-tap subpel MC kernel`

### Task D.3: Block-level `predict_inter_single_ref`

**Files:**
- Modify: `src/decoder/inter/mc.rs`

- [ ] **Step 1: `predict_inter_single_ref(block, mv, ref_frame, dst)` per plane**
- [ ] **Step 2: Chroma MV derivation** — chroma MV is luma MV scaled by subsampling
- [ ] **Step 3: Out-of-frame handling** — the reference block is extended by replication at frame edges per spec §7.11.3.5
- [ ] **Step 4: Tests**
- [ ] **Step 5: Commit** — `rustavm: block-level single-ref inter prediction`

### Task D.4: Interpolation filter selection

**Files:**
- Modify: `src/decoder/inter/mc.rs`

- [ ] **Step 1: `read_interp_filter(ctx)` — horizontal and vertical filter indices read independently when `interpolation_filter == SWITCHABLE`**
- [ ] **Step 2: Fall-through for non-switchable modes (use header default)**
- [ ] **Step 3: Wire into `predict_inter_single_ref`**
- [ ] **Step 4: Commit** — `rustavm: switchable interpolation filter selection`

---

## Phase E — Inter block decode

### Task E.1: Inter-mode block decode pipeline

**Files:**
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: Extend `decode_4x4_block` (now `decode_block(bsize, ...)`) to branch on `is_inter`**
- [ ] **Step 2: Inter branch:**
  1. Read `ref_frame` selection
  2. Read MV mode (NEAREST / NEAR / NEW / GLOBAL)
  3. Derive MVP, read MV delta if NEW
  4. Call `predict_inter_single_ref`
  5. Read residual coefficients (reuses M1's coeff reader)
  6. Dequant + inverse transform (reuses M1)
  7. Add residual to prediction
- [ ] **Step 3: Integration tests on a single-ref P-frame vector**
- [ ] **Step 4: Commit** — `rustavm: integrated single-ref inter block decode`

### Task E.2: `saved_mv_grid` for TMVP of subsequent frames

**Files:**
- Modify: `src/decoder/inter/refs.rs`
- Modify: `src/decoder/core.rs`

At frame end, save the per-8×8 MV grid onto the refreshed reference frames so later frames can do TMVP.

- [ ] **Step 1: `ReferenceFrame.saved_mvs: Arc<Vec<MotionVector>>`**
- [ ] **Step 2: Populate at frame end before calling `dpb.refresh`**
- [ ] **Step 3: Tests via a 2-frame vector: frame 0 KF, frame 1 P using TMVP**
- [ ] **Step 4: Commit** — `rustavm: save MV grid into DPB for TMVP`

---

## Phase F — Single-ref inter conformance gate

### Task F.1: Run single-ref conformance subset

- [ ] **Step 1: Populate cache**

```bash
bash tests/conformance/fetch.sh single_ref_inter
```

- [ ] **Step 2: Run tests**

```bash
cargo test -p rustavm --test conformance_test -- single_ref_inter --nocapture
```

- [ ] **Step 3: Diff-driven debugging** — for each failing vector, use `diff.rs` against libavm to localize the first diverging MC or MV. Expect this phase to dominate M2's calendar time (spec R2).

- [ ] **Step 4: Commit fixes one root-cause at a time**

### Task F.2: M2 exit checklist

- [ ] All single-ref inter conformance vectors pass
- [ ] M0 and M1 test suites still pass
- [ ] `cargo miri test -p rustavm` passes on scalar core
- [ ] `cargo clippy -p rustavm -- -D warnings` clean
- [ ] Ref frames flow as `Arc<FrameBuffer<u8>>` throughout — no raw borrows into DPB slots
- [ ] `src/decoder/inter/` has no `TODO`, `FIXME`, or `unimplemented!()`

Merge and start on M3.
