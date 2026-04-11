# M1 Full Intra Toolset Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expand the M0 walking skeleton into a decoder that passes every keyframe-only conformance vector. By the end of M1 the Rust backend handles every AV2 intra coding tool: every partition shape, every intra mode, every transform size and type, full coefficient decoding, segmentation, delta Q, skip, and multi-tile frames. Still scalar, still single-threaded, still no post-filters.

**Architecture:** Every expansion happens inside the modules created in M0 — no new top-level modules. Palette + IBC are flagged as a possible slip to M1.5 per spec risk R3. CDF tables grow from the M0 flat-init subset to the full spec tables. The partition walker becomes fully recursive (not SPLIT-only). The intra module grows from one predictor to ten. The transform module grows from one kernel to the full 4×4..64×64 × DCT/ADST/FLIPADST/IDTX/WHT set.

**Tech Stack:** Rust 2021 edition. No new external crates. Fixtures come from official AV2 KF-only conformance vectors.

**Spec reference:** `docs/superpowers/specs/2026-04-11-rustavm-pure-rust-decoder-design.md` §3 M1.

**Prerequisites:** M0 plan complete (`cargo test -p rustavm --test m0_walking_skeleton_test` green).

---

## Pre-flight: conformance corpus

- [ ] **Task 0.1: Fetch KF-only conformance vectors**

**Files:**
- Create: `tests/conformance/fetch.sh`
- Create: `tests/conformance/manifest.toml`
- Create: `tests/conformance/kf_only/` (gitignored; populated by `fetch.sh`)

The conformance suite is too large to commit. `fetch.sh` downloads the vectors into `tests/conformance/kf_only/` from the upstream AV2 location (resolves spec open-question 2 at M1 kickoff — document the exact URL in the script). `manifest.toml` lists each vector's file name, expected output MD5, and the feature set it exercises, so the test runner can select the KF-only subset.

Commit `fetch.sh`, `manifest.toml`, and `.gitignore` for the cached vectors.

```bash
git add tests/conformance/fetch.sh tests/conformance/manifest.toml tests/conformance/.gitignore
git commit -m "rustavm: add conformance vector fetch script and manifest"
```

- [ ] **Task 0.2: Conformance test runner**

**Files:**
- Create: `tests/conformance_test.rs`

A single `#[test]` per feature category that reads `manifest.toml`, filters to KF-only vectors, decodes each through the Rust backend, and compares the output MD5 against the manifest value. Skip any vector whose input file is missing (so CI on fresh checkouts doesn't hard-fail). CI nightly job populates the cache.

```rust
#[test]
fn kf_only_conformance() {
    let manifest = load_manifest("tests/conformance/manifest.toml");
    let vectors = manifest.filter_features(&["kf_only"]);
    let mut skipped = 0;
    for v in vectors {
        let path = format!("tests/conformance/kf_only/{}", v.file);
        let Ok(bytes) = std::fs::read(&path) else { skipped += 1; continue; };
        let outcome = decode_ivf_with_backend(bytes.as_slice(), BackendKind::Rust).unwrap();
        assert_eq!(outcome.md5_of_frames(), v.expected_md5, "vector {} failed", v.file);
    }
    if skipped > 0 {
        eprintln!("skipped {} KF-only vectors (cache empty)", skipped);
    }
}
```

Commit.

- [ ] **Task 0.3: Xiph smoke-test corpus and harness**

**Files:**
- Create: `tests/smoke/fetch_xiph.sh`
- Create: `tests/smoke/manifest.toml`
- Create: `tests/smoke/cache/` (gitignored)
- Create: `tests/smoke_test.rs`

Separate from the conformance harness and not a gating oracle. This is a fast CI sanity check: decode a small handful of streams from Xiph's AV2 mirror through both libavm and the Rust backend and assert the reconstructed YUV matches byte-for-byte. Catches crashes, obvious regressions, and gross divergences between runs. Does not replace the conformance-vector gate (the user is sourcing those separately).

- [ ] **Step 1: `tests/smoke/fetch_xiph.sh`**

Downloads ~5 small streams from `https://media.xiph.org/video/av2/` into `tests/smoke/cache/`. Pick streams that are fast to decode (small resolution, few frames) — the smoke test runs on every CI push, so total wall time should stay under 30 seconds.

```bash
#!/usr/bin/env bash
set -euo pipefail
BASE="https://media.xiph.org/video/av2"
CACHE="$(dirname "$0")/cache"
mkdir -p "$CACHE"
# The exact file names are discovered during Task 0.3 Step 1 — list what
# the Xiph directory actually contains at the time of fetch, pick the
# smallest 5, and hard-code the list in this script. Commit the script
# once the list is pinned.
FILES=(
    # e.g. "short_clip_1.ivf"
    # e.g. "short_clip_2.ivf"
)
for f in "${FILES[@]}"; do
    if [ ! -f "$CACHE/$f" ]; then
        curl -fsSL "$BASE/$f" -o "$CACHE/$f"
    fi
done
```

- [ ] **Step 2: `tests/smoke/manifest.toml`**

Lists each fetched file with a short descriptor. No expected MD5 — the expectation is "Rust output == libavm output," computed at test time.

```toml
[[stream]]
file = "short_clip_1.ivf"
description = "Xiph AV2 mirror, smallest clip"

[[stream]]
file = "short_clip_2.ivf"
description = "Xiph AV2 mirror, second-smallest"
```

- [ ] **Step 3: `tests/smoke_test.rs`**

```rust
//! Xiph smoke test: decode each pinned stream through both backends and
//! assert byte-identical YUV. Not a gating oracle — the real gate is the
//! conformance vector suite in `tests/conformance_test.rs`. This test
//! catches crashes and gross regressions fast.

use rustavm::{decode_ivf_with_backend, BackendKind};

#[derive(serde::Deserialize)]
struct Manifest { stream: Vec<Stream> }

#[derive(serde::Deserialize)]
struct Stream { file: String, description: String }

fn load_manifest() -> Manifest {
    let text = std::fs::read_to_string("tests/smoke/manifest.toml").expect("manifest");
    toml::from_str(&text).expect("parse manifest")
}

#[test]
fn xiph_smoke_rust_matches_libavm() {
    let manifest = load_manifest();
    let mut ran = 0;
    let mut skipped = 0;
    for s in manifest.stream {
        let path = format!("tests/smoke/cache/{}", s.file);
        let Ok(bytes) = std::fs::read(&path) else {
            skipped += 1;
            continue;
        };
        let libavm_out = decode_ivf_with_backend(bytes.as_slice(), BackendKind::Libavm)
            .unwrap_or_else(|e| panic!("libavm decode of {} failed: {e:?}", s.file));
        let rust_out = decode_ivf_with_backend(bytes.as_slice(), BackendKind::Rust)
            .unwrap_or_else(|e| panic!("rust decode of {} failed: {e:?} ({})", s.file, s.description));
        assert_eq!(
            rust_out.yuv(), libavm_out.yuv(),
            "YUV mismatch on {}: {}", s.file, s.description,
        );
        ran += 1;
    }
    if skipped > 0 {
        eprintln!("xiph smoke: ran {ran}, skipped {skipped} (cache empty — run tests/smoke/fetch_xiph.sh)");
    }
    assert!(ran > 0 || skipped > 0, "smoke manifest was empty");
}
```

`serde` and `toml` are likely already dev-deps from Task 0.2's manifest parsing; if not, add them as dev-deps in this step.

- [ ] **Step 4: Wire into CI**

The smoke test runs on every CI push. The fetch script runs as a pre-step and caches into `tests/smoke/cache/`. Because `cache/` is gitignored, CI needs to either (a) run `fetch_xiph.sh` on every build or (b) use a CI-level cache action keyed on the pinned file list. Prefer (b) to avoid hammering Xiph.

Add (or modify) a GitHub Actions workflow step:

```yaml
- name: Cache Xiph smoke corpus
  uses: actions/cache@v4
  with:
    path: tests/smoke/cache
    key: xiph-av2-smoke-v1

- name: Fetch Xiph smoke corpus
  run: bash tests/smoke/fetch_xiph.sh

- name: Run smoke test
  run: cargo test -p rustavm --test smoke_test
```

If the project doesn't use GitHub Actions, translate the three steps to whichever CI is in use.

- [ ] **Step 5: Commit**

```bash
git add tests/smoke/ tests/smoke_test.rs .github
git commit -m "rustavm: add Xiph smoke test harness for backend divergence detection"
```

**Important scope notes:**
- The smoke test is **not** the M1 exit gate. Conformance vectors still are (Task I.1). If Xiph streams exercise features M1 hasn't implemented yet (most likely inter — Xiph's corpus has plenty of inter-frame content), filter the manifest to pick clips whose content is within M1's capability. As later milestones land, expand the manifest to add clips that exercise their features.
- Failures in Xiph smoke do not always indicate a Rust-backend bug. libavm has its own bugs. If a stream diverges and the divergence is traceable to libavm misbehavior, add the stream to an `xfail` list in the manifest with a comment pointing at the libavm issue, rather than silencing the test or flipping the assertion.
- This test is preserved past M7. The spec's M7 FFI-removal plan keeps the libavm backend behind `--features libavm-diff`; the smoke test runs under that feature and remains a CI job forever.

---

## Phase A — Frame header completion

The M0 parser handles a fraction of the KF frame header. M1 finishes it. Every field that affects KF decoding must parse correctly.

### Task A.1: Expand `UncompressedFrameHeader`

**Files:**
- Modify: `src/bitstream.rs`

Add all remaining fields needed for KF decoding: `error_resilient_mode`, `disable_cdf_update`, `allow_screen_content_tools`, `force_integer_mv`, `render_size`, `superres_params`, full `loop_filter_params`, full `quant_params` (ac_delta_q, dc_delta_q for each plane, quantizer matrices), `segmentation_params`, `delta_q_params`, `delta_lf_params`, `loop_restoration_params` (parsed but unused until M4), `tx_mode`, `reduced_tx_set`, `cdef_params` (parsed but unused until M4), `film_grain_params` (parsed but unused until M4).

- [ ] **Step 1: Port the frame-header structure fields from AV2 spec §5.9.1**
- [ ] **Step 2: Port the parser branch-by-branch, KF path only**
- [ ] **Step 3: Unit tests** — one test per conformance KF vector (or a small, curated subset) that asserts the parse result against hand-extracted ground truth.
- [ ] **Step 4: Commit** — `rustavm: parse full uncompressed KF frame header`

### Task A.2: Quant parameter application

**Files:**
- Modify: `src/decoder/quant.rs`

Replace the M0 fixed-QP dequant with a `QuantContext` that holds per-plane DC/AC deltas, QM indices, and delta-Q per-segment / per-block offsets. Expose `dequant_block(&self, tx_size, plane, qm_idx, coeffs_in, coeffs_out)`.

- [ ] **Step 1: `QuantContext` struct and constructor from `UncompressedFrameHeader`**
- [ ] **Step 2: `dequant_block` for all transform sizes — scales from the spec §7.12 tables**
- [ ] **Step 3: Quantization matrices — port `QM_TABLE` from spec §7.12.3, gate on `using_qmatrix`**
- [ ] **Step 4: KAT tests against libavm-produced reference dequant outputs for a handful of (qindex, qm_idx, tx_size) tuples**
- [ ] **Step 5: Commit** — `rustavm: full quant context with per-plane delta and QM`

---

## Phase B — Partition tree

### Task B.1: Full partition enum and CDF wiring

**Files:**
- Modify: `src/decoder/partition.rs`
- Modify: `src/decoder/symbols.rs`
- Modify: `src/decoder/entropy.rs`

Replace the M0 `PartitionType::{None, Split}` with the full AV2 set: NONE, HORZ, VERT, SPLIT, HORZ_A, HORZ_B, VERT_A, VERT_B, HORZ_4, VERT_4. The exact variant set available at each block size is defined by spec §6.4.

- [ ] **Step 1: Extend the `PartitionType` enum**
- [ ] **Step 2: Port partition CDFs from spec §9.3 into `symbols.rs`** — one CDF per (block_size, context) pair.
- [ ] **Step 3: `BacReader::read_partition(bsize, ctx)` wrapper**
- [ ] **Step 4: Unit tests** — decode a synthetic bitstream against a known partition sequence.
- [ ] **Step 5: Commit** — `rustavm: full partition type set with per-bsize CDFs`

### Task B.2: Recursive walker over every partition shape

**Files:**
- Modify: `src/decoder/partition.rs`

Replace `walk_sb_split_only` with a general walker that takes a `Fn(BlockSize, PartitionType) -> ...` partition-decision closure. Each shape dispatches the sub-block iteration differently:

- HORZ / VERT: two child rectangles
- HORZ_A / HORZ_B / VERT_A / VERT_B: three children
- HORZ_4 / VERT_4: four children
- NONE: single block, no recursion

- [ ] **Step 1: Shape-by-shape dispatch table**
- [ ] **Step 2: Table-driven unit tests** — one per shape, asserting exact sub-block coordinates
- [ ] **Step 3: Commit** — `rustavm: recursive partition walker for every partition shape`

### Task B.3: Neighbor context tracking

**Files:**
- Modify: `src/decoder/partition.rs`
- Create: `src/decoder/block_info.rs`

Every block decision depends on neighbor context (above + left). Introduce a `BlockInfoGrid` that stores per-4×4 mode/skip/tx_size so neighbor lookups work.

- [ ] **Step 1: `BlockInfoGrid` with per-4×4 entries sized to frame dimensions**
- [ ] **Step 2: `ctx_above(x, y, bsize)` / `ctx_left(x, y, bsize)` helpers**
- [ ] **Step 3: Unit tests** — verify context at frame boundaries and mid-frame
- [ ] **Step 4: Wire into `partition.rs` recursion** — each block decision gets its context from the grid before calling BAC.
- [ ] **Step 5: Commit** — `rustavm: block-info grid and neighbor context`

---

## Phase C — CDFs (mechanical port)

### Task C.1: Port all KF-reachable CDF tables from spec §9

**Files:**
- Modify: `src/decoder/symbols.rs`
- Create: `tools/gen_cdfs.py` (optional; see spec risk R1)

The flat-init placeholders in M0 must be replaced with the real spec tables for every CDF the KF path touches: partition, intra_mode_y, intra_mode_uv, skip, segment_id, segment_id_predicted, delta_q, delta_lf, filter_intra, filter_intra_mode, cfl_alpha_sign, cfl_alpha_index, tx_size, tx_type, coeff_base_eob, coeff_base, coeff_br, dc_sign, eob_pt_16/32/64/128/256/512/1024.

- [ ] **Step 1: If the spec ships machine-readable tables, write `tools/gen_cdfs.py` to emit a Rust module with the tables**
- [ ] **Step 2: Otherwise, hand-port one table at a time** — each commit is one symbol family
- [ ] **Step 3: For each CDF, add a unit test that hashes the table bytes** — this guards against typos during the port
- [ ] **Step 4: Commit** — one commit per CDF family, e.g. `rustavm: port intra_mode_y CDFs from AV2 §9.3`

### Task C.2: CDF adaptation

**Files:**
- Modify: `src/decoder/symbols.rs`

AV2 CDFs update after every symbol read (unless `disable_cdf_update=1`). Implement the per-symbol count-based update formula from spec §5.11.39.

- [ ] **Step 1: `CdfState<const N: usize>` wrapper with `update(&mut self, symbol: usize)`**
- [ ] **Step 2: KAT tests** — start from initial CDF, read 10 symbols from a fixture, assert final CDF matches libavm's post-update state
- [ ] **Step 3: Commit** — `rustavm: CDF adaptation update rule`

### Task C.3: Per-tile CDF reset

**Files:**
- Modify: `src/decoder/core.rs`
- Modify: `src/decoder/symbols.rs`

At the start of every tile, CDFs reset to either (a) the default init tables or (b) the state from the `primary_ref_frame`. KF always resets to defaults.

- [ ] **Step 1: `TileContext` struct owning a `CdfState` for every symbol family**
- [ ] **Step 2: `TileContext::new_default()` for KF path**
- [ ] **Step 3: Wire into tile dispatch in `core.rs`**
- [ ] **Step 4: Commit** — `rustavm: per-tile CDF reset for KF path`

---

## Phase D — Transforms

### Task D.1: Every transform size and type

**Files:**
- Modify: `src/decoder/kernels/scalar.rs`
- Modify: `src/decoder/transform.rs`

The AV2 transform set is all pairs of 1D transforms (DCT, ADST, FLIPADST, IDTX, WHT) applied to sizes 4×4, 8×8, 16×16, 32×32, 64×64, plus rectangular 4×8/8×4/8×16/…/32×64. Full matrix is ~50 kernels.

**Strategy:** Start with square sizes + DCT/ADST/FLIPADST/IDTX combos — the most common path. Add rectangular sizes next. Add WHT last (rare).

- [ ] **Step 1: 1D inverse DCT kernels for sizes 4, 8, 16, 32, 64**
- [ ] **Step 2: 1D inverse ADST kernels for sizes 4, 8, 16, 32**
- [ ] **Step 3: 1D inverse FLIPADST = ADST with input reversal**
- [ ] **Step 4: IDTX = identity transform (passthrough with rounding)**
- [ ] **Step 5: WHT 4×4**
- [ ] **Step 6: 2D inverse transform dispatcher** — applies 1D along rows, then 1D along columns. Lookup table keyed on `(TxSize, TxType)`.
- [ ] **Step 7: Rectangular sizes** — rows use the wide dim, columns use the tall dim
- [ ] **Step 8: KAT tests per (size, type)** — each commit adds one family's KATs against libavm reference outputs
- [ ] **Step 9: Commit progression** — ~10 commits, one per family

### Task D.2: Transform-size selection and signaling

**Files:**
- Modify: `src/decoder/transform.rs`

- [ ] **Step 1: Port `tx_mode` interpretation from spec §6.4.2** (ONLY_4X4, LARGEST, SELECT)
- [ ] **Step 2: `read_tx_size` for the SELECT mode — recursive tx partitioning**
- [ ] **Step 3: Tests with known bitstreams**
- [ ] **Step 4: Commit** — `rustavm: tx_mode selection and recursive tx_size signaling`

---

## Phase E — Intra prediction

### Task E.1: Directional intra modes

**Files:**
- Modify: `src/decoder/intra.rs`
- Modify: `src/decoder/kernels/scalar.rs`

AV2 directional intra has 8 base directions × 7 nominal angle offsets = 56 angles. All share a common "directional predictor" kernel parameterized by angle.

- [ ] **Step 1: `predict_directional(angle, bsize, above, left, dst, stride)` — the generic kernel**
- [ ] **Step 2: Reference sample gathering** — above row includes `2 * bsize + 1` samples; left column same
- [ ] **Step 3: Intra edge filter** (spec §7.11.2.4) — applied to reference samples before prediction
- [ ] **Step 4: Intra edge upsample** (spec §7.11.2.5) — for small blocks at sharp angles
- [ ] **Step 5: Angle-specific interpolation inside `predict_directional`**
- [ ] **Step 6: KATs for a dozen (angle, bsize) pairs**
- [ ] **Step 7: Commit progression** — 3–4 commits

### Task E.2: Non-directional modes

**Files:**
- Modify: `src/decoder/intra.rs`

- [ ] **Step 1: SMOOTH, SMOOTH_H, SMOOTH_V predictors** — bilinear interpolation of reference edges
- [ ] **Step 2: Paeth predictor** (spec §7.11.3.2)
- [ ] **Step 3: CFL (chroma-from-luma)** (spec §7.11.5) — uses the reconstructed luma block
- [ ] **Step 4: Recursive-filter intra** (spec §7.11.4) — 5 filter sets, each applies an intra filter via a lookup table
- [ ] **Step 5: DC (already in M0) adapted to variable block sizes**
- [ ] **Step 6: KATs per mode**
- [ ] **Step 7: Commit** — `rustavm: full non-directional intra mode set`

### Task E.3: Palette and IBC (may slip to M1.5 per spec R3)

**Files:**
- Modify: `src/decoder/intra.rs`
- Create: `src/decoder/palette.rs`

- [ ] **Step 1: Palette token decode** (spec §5.11.46 + §7.11.4.2) — per-block palette, color cache, run-length token
- [ ] **Step 2: Intra-block-copy** (spec §7.11.6) — uses an MV into the currently-reconstructed area, constrained to the same tile
- [ ] **Step 3: KAT and conformance checks on screen-content vectors**
- [ ] **Step 4: Commit** — `rustavm: palette and IBC for screen-content streams`

**If this task slips:** filter `manifest.toml` to exclude screen-content KF vectors, land M1 without palette/IBC, then reopen this task as M1.5 before starting M2.

---

## Phase F — Coefficients

### Task F.1: Full 4×4..64×64 coefficient reader

**Files:**
- Modify: `src/decoder/entropy.rs`

AV2 coefficient coding is the most context-heavy subsystem in the KF path. Spec §5.11.39 is the source of truth.

- [ ] **Step 1: `read_coeffs(tx_size, tx_type, plane, ctx, out)` signature**
- [ ] **Step 2: EOB decoding** — `eob_pt_N` CDFs per transform size plus a subexp refinement (spec §5.11.39.3)
- [ ] **Step 3: Scan orders** — port `SCAN_4X4`, `SCAN_8X8`, …, `SCAN_64X64` from spec §9.4 (iZigZag, diagonal, row, column scans)
- [ ] **Step 4: Base level decode** — `coeff_base` CDF with neighbor-context lookup, generates levels 0–3
- [ ] **Step 5: Bitwise refinement** — `coeff_br` CDF for levels >3, followed by `read_golomb` for very large levels
- [ ] **Step 6: DC sign context** — per spec §5.11.39.11
- [ ] **Step 7: Other-coeff sign** — plain bit read
- [ ] **Step 8: Context computation** — the trickiest piece. Per-position neighbor-sum context via the `get_ctx` formulas in spec §5.11.39.8. Hand-port and test against libavm on synthetic blocks.
- [ ] **Step 9: Tests** — a suite of hand-crafted blocks with known coefficient patterns, decoded through both libavm and the Rust reader, compared block-by-block
- [ ] **Step 10: Commit progression** — ~6 commits, one per sub-stage

### Task F.2: Dequant-to-transform integration

**Files:**
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: Wire `read_coeffs` → `dequant_block` → `inverse_transform` → add-to-pred**
- [ ] **Step 2: Integration test** against a KF fixture with non-zero residuals
- [ ] **Step 3: Commit** — `rustavm: non-zero residual path integrated in tile decode`

---

## Phase G — Segmentation, delta Q, skip

### Task G.1: Segmentation

**Files:**
- Modify: `src/decoder/core.rs`
- Modify: `src/decoder/quant.rs`
- Modify: `src/decoder/partition.rs`

- [ ] **Step 1: `SegmentationContext` built from `SegmentationParams` in the frame header** — 8 segments, per-segment feature enables for Q, LF, REF, SKIP
- [ ] **Step 2: Per-block segment_id read + prediction** (spec §5.11.9)
- [ ] **Step 3: Per-block Q-delta application** — modifies the qindex fed to `dequant_block`
- [ ] **Step 4: Segment skip feature** — forces skip=true when the segment has FEATURE_SKIP set
- [ ] **Step 5: Commit** — `rustavm: segmentation with Q and skip features`

### Task G.2: Delta Q and delta LF

**Files:**
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: Per-block delta_q signaling** (spec §5.11.38) — updates a running q offset
- [ ] **Step 2: Per-block delta_lf signaling** — stored for M4 (loop filter) but parsed now
- [ ] **Step 3: Tests on vectors that exercise delta Q**
- [ ] **Step 4: Commit** — `rustavm: delta_q and delta_lf signaling`

### Task G.3: Skip flag propagation

**Files:**
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: `read_skip(ctx)` wired into block decode** — when skip=1, all coefficient planes are zero and transform dispatch is bypassed
- [ ] **Step 2: Tests**
- [ ] **Step 3: Commit** — `rustavm: skip flag propagation`

---

## Phase H — Multi-tile frames

### Task H.1: Multi-tile support in the frame driver

**Files:**
- Modify: `src/bitstream.rs`
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: Parse `tile_info` completely** (spec §5.9.15) — tile_cols_log2, tile_rows_log2, tile_col_start, tile_row_start
- [ ] **Step 2: Parse multi-tile OBUs** — one tile group OBU can carry multiple tiles with size fields per tile
- [ ] **Step 3: Dispatch each tile through the existing `decode_tile` via `TileExecutor::for_each_tile`**
- [ ] **Step 4: Per-tile entropy state isolation** — each tile gets its own `BacReader` and its own `TileContext` (CDFs reset at tile boundary)
- [ ] **Step 5: Tile-boundary neighbor blocking** — blocks at tile boundaries see `None` for out-of-tile neighbors
- [ ] **Step 6: Tests** — conformance vectors with multi-tile frames
- [ ] **Step 7: Commit** — `rustavm: multi-tile frame decode via TileExecutor`

---

## Phase I — KF-only conformance gate

### Task I.1: Run the full KF-only conformance subset

- [ ] **Step 1: Populate the conformance cache**

```bash
bash tests/conformance/fetch.sh kf_only
```

- [ ] **Step 2: Run the test**

```bash
cargo test -p rustavm --test conformance_test -- kf_only_conformance --nocapture
```

Expected: every vector passes or the reason for failure is a named subsystem with a follow-up task. No silent skips. No TODOs.

- [ ] **Step 3: Fix any remaining vector failures**

Use `src/diff.rs` against libavm to localize the first failing frame, tile, SB, and block.

- [ ] **Step 4: Commit each fix as its own commit** — one per root cause, not per vector

### Task I.2: M1 exit checklist

- [ ] All KF-only conformance vectors pass (`cargo test -p rustavm --test conformance_test kf_only`).
- [ ] `cargo test -p rustavm` passes clean.
- [ ] `cargo miri test -p rustavm` passes on the scalar core (excluding conformance vectors that read from disk).
- [ ] `cargo clippy -p rustavm -- -D warnings` clean.
- [ ] Palette + IBC: either landed (Task E.3) or explicitly deferred to M1.5 with the manifest filtered accordingly.
- [ ] M0 integration test still passes.
- [ ] `src/decoder/` contains no `TODO`, `FIXME`, or `unimplemented!()`.

When all boxes are checked, merge and start on M2.
