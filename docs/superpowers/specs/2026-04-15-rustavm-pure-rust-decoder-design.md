# rustavm: pure-Rust AV2 decoder — design

**Date:** 2026-04-15 (updated from 2026-04-11)
**Audience:** collaborators onboarding onto the pure-Rust decoder workstream. Assumes Rust fluency but not deep AV2 internals.
**Status:** M0 complete (with caveats); M1 in progress.
**How to use this doc.** This is the end-to-end design for the entire M0–M7 conversion. It is deliberately too large to execute as a single implementation plan. Each milestone (M0, M1, …) gets its own implementation plan written against this spec, in order.

> **Changelog from 2026-04-11 original:**
> - §1.2: `decoder.rs` replaced by `decoder/mod.rs` (rename reflected).
> - §1.3: Completely rewritten to reflect M0 completion and post-M0 frame-header work.
> - §2: All subsystem status entries updated to current state.
> - §5.2: Open questions updated — Q2 is actively in progress.

---

## 0. AV2 primer for collaborators

A one-screen crash course so the rest of this doc is legible. Skip if you already decode video for a living.

- **OBU (Open Bitstream Unit).** AV2 streams are a sequence of OBUs: sequence header, temporal delimiter, frame header, tile group, metadata, etc. Each OBU has a 1-byte header (plus optional extension byte) and an optional size field. `src/bitstream.rs` parses this layer.
- **Sequence header.** Stream-level parameters: profile, bit depth, subsampling (4:2:0 / 4:2:2 / 4:4:4), timing, operating points, whether film grain is present, etc. Stable across the stream.
- **Frame header (uncompressed).** Per-frame parameters: frame type (KEY / INTER / INTRA_ONLY / SWITCH), reference frame selection, quant params, loop-filter params, segmentation, CDEF params, loop-restoration params, film-grain params, etc. This is where most of the complexity lives.
- **Tile group OBU.** Carries the entropy-coded payload for one or more tiles. Tiles are the unit of parallelism: independent entropy state, independent decoding (mostly).
- **Inside a tile.** The frame is tiled; each tile is a grid of **superblocks** (SBs, usually 128×128 or 64×64); each SB recursively partitions into **blocks** down to 4×4; each block has a prediction mode, optionally motion vectors, optionally transform coefficients, and reconstructs itself.
- **Pipeline per block.** (1) Entropy-decode syntax → (2) predict (intra or inter) → (3) dequant + inverse transform on residual coefficients → (4) add residual to prediction → write pixels.
- **Post-filters (in order).** After all tiles reconstruct: deblocking → CDEF (constrained directional enhancement filter) → LR (loop restoration: Wiener + self-guided) → film grain synthesis on the output image.
- **Reference frames / DPB.** Up to 8 slots. Each frame can refresh some subset; inter frames pick references by index, with order-hint-aware ref-list construction. Getting this wrong silently corrupts every subsequent frame.

That's the whole vertical. The rest of this doc is "how do we build each piece in Rust, in what order, and how do we know it's right."

## 1. Scope, non-goals, module layout

### 1.1 Scope

Replace `BackendKind::Libavm` with a pure-Rust AV2 decoder that:

1. Passes the official AV2 conformance vectors for the Main profile, end-to-end.
2. Runs within ~10% of libavm on a representative corpus (see §5.2 open question 3).
3. Preserves the existing `Decoder` / `DecoderBuilder` public API unchanged. The backend swap is invisible to callers.

**In scope.**

- All AV2 Main-profile coding tools exercised by the conformance vectors.
- 8-bit and 10-bit 4:2:0 unconditionally. 4:2:2, 4:4:4, and 12-bit only if conformance vectors exercise them. See M4 for the actual landing milestone.
- Film grain synthesis.
- Still-picture mode, reduced still-picture header, multi-layer (temporal/spatial) as exercised by vectors.
- Tile parallelism behind a `TileExecutor` trait seam.
- Scalar kernels from day one; SIMD impls land in M5 behind the `Kernels` trait.
- Frame-buffer-manager interop: the existing `FrameBufferManager` public trait must keep working.

**Non-goals.**

- The encoder. Ever.
- `no_std` or WASM targets.
- Perf work beyond what's needed to hit the ~10% target (no GPU offload, no async frame decode, no speculation).
- Bit-exact compatibility with non-conformant libavm behavior. When libavm and the spec disagree, the spec wins — the differential harness is a dev tool, not an oracle.

### 1.2 Target module layout

New pure-Rust code lives under `src/decoder/`. Top-level modules stay where they are.

```
src/
  backend.rs                    # unchanged
  backend/
    libavm.rs                   # kept in tree M0–M6, deleted in M7
    rust.rs                     # thin shell; forwards into decoder::core
  bitstream.rs                  # OBU + headers; grows to full uncompressed header
  decoder/
    mod.rs                      # existing public API surface; unchanged
                                # (was decoder.rs pre-M0; renamed to mod.rs during M0)
    core.rs                     # top-level decode loop, frame/tile driver
    entropy.rs                  # boolean arithmetic coder
    symbols.rs                  # CDF tables + init/update
    partition.rs                # SB → partition → block tree
    intra.rs                    # intra prediction modes + recon
    inter/
      mod.rs                    # inter entry
      mv.rs                     # MV derivation, ref-list construction
      mc.rs                     # subpel MC filters, compound, warped, OBMC
      refs.rs                   # reference-frame manager (DPB)
    transform.rs                # inverse transforms (table-driven outer layer)
    quant.rs                    # dequant + QM
    loopfilter.rs               # deblocking (outer layer)
    cdef.rs                     # CDEF (outer layer)
    restoration.rs              # Wiener + self-guided LR (outer layer)
    film_grain.rs               # film grain synthesis
    frame_buffer.rs             # reconstructed frame storage + ref management
    kernels/
      mod.rs                    # Kernels trait; runtime dispatch
      scalar.rs                 # portable scalar impls
      simd_x86.rs               # M5: AVX2, AVX-512 where it helps
      simd_aarch64.rs           # M5: NEON
    executor.rs                 # TileExecutor trait; single-threaded default
    threaded.rs                 # M6: parallel tile executor
  diff.rs                       # dev tool throughout; decision at M7
  stream.rs, format.rs, ivf.rs  # unchanged
```

Each file has one clear purpose, communicates through a typed interface, and stays small enough to review in one sitting. Files that grow past ~800 LOC are a signal to split.

### 1.3 Current implementation state (as of 2026-04-15)

HEAD is `ad234a97` on `main`. M0 is functionally complete with the caveats noted below.

**What exists and works:**

- `BackendKind { Libavm, Rust }` enum with `Default = Libavm`. `DecoderBuilder::backend()` selects the pure-Rust path; libavm remains the default (changes to `Rust` in M7).
- `src/backend/libavm.rs` — full FFI wrapper, `FrameBufferManager` interop, shims.
- `src/backend/rust.rs` — production thin-shell. Parses OBUs, extracts full sequence header and uncompressed frame header, delegates frame decode to `src/decoder/core.rs`, returns `avm_image_t` output.
- `src/bitstream.rs` (~2340 LOC) — full OBU framing, Annex-B support, `parse_obus_auto`; comprehensive sequence header (profile, bit depth, color config, timing, operating points, toolset flags); uncompressed frame header covering the KF path in full: frame type, show_frame, quant params (base + delta + qmatrix), loop filter params, delta-Q, tx_mode, reduced_tx_set, frame size (with override), screen content tools, force_integer_mv; `split_frame_obu_payload` returning the header and tile payload bytes at the correct boundary; tile group OBU.
- `src/decoder/frame_buffer.rs` — `Pixel` trait (`u8`; `u16` stub for M4), `PlaneBuffer<P>` with 64-byte-aligned stride, `FrameBuffer<P>` for Y/U/V.
- `src/decoder/kernels/mod.rs` + `scalar.rs` — `Kernels` trait with `detect()` runtime dispatch, scalar impl, inverse DCT4×4.
- `src/decoder/executor.rs` — `TileExecutor` trait, `Sequential` default.
- `src/decoder/entropy.rs` — BAC reader (init, read_bool_unbiased, read_symbol, plus thin wrappers for partition, skip, intra_mode, coeffs_4x4).
- `src/decoder/symbols.rs` — CDF tables for the M0 symbol set (uniform/stub distributions — not yet spec-derived values).
- `src/decoder/quant.rs` — `QuantContext::from_frame_header`, `dequant_4x4(Plane, ...)`, full AC/DC 8-bit lookup tables.
- `src/decoder/transform.rs` — inverse transform dispatch (DCT4×4 only).
- `src/decoder/intra.rs` — DC intra predictor for variable block sizes.
- `src/decoder/partition.rs` — 64×64 SB (`BlockSize::SB_M0`), recursive SPLIT-only partition walker.
- `src/decoder/core.rs` — full decode loop wiring: frame → tile → SB → block. Accepts both reduced-still-picture and general keyframe streams. Hard gates: 8-bit only, single-tile only, SPLIT-only partition.
- `src/diff.rs` + `tests/differential_test.rs` — bit-exact differential harness (Rust vs libavm on any IVF stream).
- `tests/corpora/m0/` — committed oracle fixture (`dc_intra_4x4.ivf` + `dc_intra_4x4.expected.yuv`) and secondary fixture.
- `tests/m0_walking_skeleton_test.rs` — bit-exact round-trip against libavm.
- `tests/decode_api_test.rs` — covers Rust backend through all general keyframe header variants (lossless, non-lossless, deblocking, delta-Q, qmatrix, qmatrix split-UV).
- C tools in `tools/` for fixture generation and entropy debugging.
- `avmdec --backend rust` / `--compare-backend` / `--compare-outcomes` CLI flags.

**Known M0 caveats (to be resolved in M1):**

- `read_coeffs_4x4` returns `Err(EntropyError::UnimplementedInM0)` for any non-zero coefficient block. The M0 corpus is constructed to contain only all-zero blocks.
- CDF tables in `symbols.rs` are uniform placeholder distributions, not spec-derived values.
- Partition walker is SPLIT-only; no HORZ / VERT / compound partition types.
- DC intra only; no other prediction modes.
- DCT4×4 only; no other transform sizes or types.
- Single-tile only.
- 8-bit only (10-bit guard in `decode_frame`).
- `order_hint` is hardcoded to 0.
- Many uncompressed frame header fields are parsed but not yet consumed by the decode pipeline: superres, segmentation, delta_lf, CDEF, loop restoration, film grain.

**Open code review findings** (see `docs/reviews/2026-04-15-m0-code-review.md`):
- Critical: `src/backend/libavm.rs` — all `unsafe` blocks missing `SAFETY:` comments.
- Important: 7 items, mostly documentation and safety-comment gaps; see the review doc for the full list.

---

## 2. Subsystem catalog

Every piece the pure-Rust decoder needs, grouped by layer. "Status" reflects the repo as of 2026-04-15; "owner" is the target module from §1.2.

### 2.1 Bitstream layer

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| OBU framing (split, header, extension, size) | ✅ done | `bitstream.rs` | Low-overhead and Annex-B both supported via `parse_obus_auto`. |
| Sequence header | ✅ done | `bitstream.rs` | Reads profile, bit depth, subsampling, color config, operating points, timing info present flag, toolset flags (`force_screen_content_tools`, `force_integer_mv`, `enable_cdef`, `enable_restoration`, etc.), delta-Q base offsets, `film_grain_params_present`, `df_par_bits_minus2`. |
| Uncompressed frame header (KF path) | ⚠ partial | `bitstream.rs` | Reads: frame_type, show_frame, error_resilient_mode, disable_cdf_update, primary_ref_frame, refresh_frame_flags, frame_size_override, allow_screen_content_tools, force_integer_mv, quant_params (full: base + all deltas + qmatrix), loop_filter_params, delta_q, tx_mode, reduced_tx_set. Still missing for M1: order_hint, superres params, segmentation fields (currently hardcoded disabled), delta_lf, CDEF params, loop restoration params, film grain params. Inter-frame fields (ref_frame_idx, ref_order_hint, MV etc.) deferred to M2. |
| Tile group OBU | ✅ done | `bitstream.rs` | `parse_tile_group` extracts `tile_start` / `tile_end` and payload. Currently single-tile only; multi-tile for M1. |

### 2.2 Entropy + symbols

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Boolean arithmetic coder | ✅ done | `entropy.rs` | `BacReader`: init, `read_bool_unbiased`, `read_symbol`. Spec-correct range/value update. |
| CDF tables + init / update | ⚠ stub | `symbols.rs` | Tables exist for the M0 symbol set (partition, skip, intra_mode, all_zero) but use uniform placeholder distributions, not spec-derived values. Adaptation / per-tile CDF reset not yet implemented. |
| Symbol readers (per-syntax-element) | ⚠ partial | `entropy.rs` | M0 path only: `read_partition_none_or_split`, `read_skip`, `read_intra_mode`, `read_coeffs_4x4` (all-zero path; non-zero returns `UnimplementedInM0`). M1 adds the full coefficient decode path and all remaining intra symbol readers. |

### 2.3 Block decode

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Superblock / partition tree | ⚠ partial | `partition.rs` | `BlockSize::SB_M0` is 64×64. Recursive SPLIT-only walker. NONE / HORZ / VERT / compound partition types are M1. |
| Block info (mode_info) propagation | ❌ missing | `partition.rs` | Above / left neighbor contexts for mode and MV prediction. M1. |
| Segmentation | ❌ missing | `partition.rs` | Per-block seg_id, feature deltas. M1. |
| Skip / skip_mode | ⚠ partial | `entropy.rs` + `core.rs` | `read_skip` exists; skip dispatches to the all-zero coefficient path. Full skip_mode (M3) and segmentation-driven skip deferred. |
| Palette + intra-block-copy | ❌ missing | `intra.rs` | Screen-content tools. M1 / M1.5 per risk R3. |

### 2.4 Coefficients & reconstruction

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Coefficient decoding | ⚠ stub | `entropy.rs` | All-zero path implemented. Non-zero path returns `UnimplementedInM0`. Full EOB / base / br / sign / coeff_context decode is the core M1 work item. |
| Dequantization + QM | ✅ done | `quant.rs` | `QuantContext::dequant_4x4(Plane, in, out)`. Full 8-bit AC/DC lookup tables. Per-plane delta application. QMatrix flag parsed and stored (QM index tables not yet applied to reconstruction — no non-zero coefficients in M0 anyway). |
| Inverse transforms | ⚠ partial | `transform.rs` + `kernels/` | DCT4×4 scalar implemented and wired. All other sizes and transform types (ADST, FLIPADST, IDTX, WHT, rectangular) are M1. |

### 2.5 Prediction

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Intra modes | ⚠ partial | `intra.rs` | DC intra predictor for variable block sizes. All other modes (directional, SMOOTH*, Paeth, CFL, recursive filters) are M1. |
| Intra edge filter + upsample | ❌ missing | `intra.rs` | M1. |
| MV derivation | ❌ missing | `inter/mv.rs` | M2. |
| Reference frame manager (DPB) | ❌ missing | `inter/refs.rs` | M2. |
| Subpel MC filters | ❌ missing | `inter/mc.rs` + `kernels/` | M2. |
| Compound prediction | ❌ missing | `inter/mc.rs` | M3. |
| Warped motion + global motion | ❌ missing | `inter/mc.rs` + `kernels/` | M3. |
| OBMC | ❌ missing | `inter/mc.rs` | M3. |

### 2.6 Post-filters

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Deblocking | ❌ missing | `loopfilter.rs` + `kernels/` | Loop filter params are now parsed from the frame header; application is M4. |
| CDEF | ❌ missing | `cdef.rs` + `kernels/` | M4. |
| Loop restoration | ❌ missing | `restoration.rs` + `kernels/` | M4. |
| Film grain synthesis | ❌ missing | `film_grain.rs` | M4. |

### 2.7 Frame lifecycle

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Reconstructed frame storage | ✅ done | `frame_buffer.rs` | `Pixel` trait (`u8` impl; `u16` stub for M4), `PlaneBuffer<P>` with 64-byte-aligned stride, `FrameBuffer<P>` owning Y/U/V planes. |
| Output queue | ⚠ partial | `decoder/mod.rs` | Single-frame show_frame path works. `show_existing_frame` / `showable_frame` ordering deferred; `tests/differential_test.rs` exercises this against libavm for future use. |
| Error model | ⚠ partial | `decoder/mod.rs` | `DecoderError` exists with Unimplemented / BackendError / ParseError variants. Conformance-violation vs. unsupported-tool distinction still needed for M1+. |

### 2.8 Perf & concurrency seams

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| `Kernels` trait | ✅ done | `decoder/kernels/mod.rs` | `Kernels::detect() -> &'static dyn Kernels` runtime dispatch. `inv_dct4x4` is the first method. Scalar impl in `scalar.rs`. SIMD slots are M5. |
| `TileExecutor` trait | ✅ done | `decoder/executor.rs` | `TileExecutor::for_each_tile` trait, `Sequential` default. Parallel impl is M6. |
| FFI compat layer | ✅ done | `backend/rust.rs` | Produces `avm_image_t` via `zeroed_image()` + plane pointer fill. Public API unchanged. |

---

## 3. Milestone plan

Each milestone exits on passing a curated subset of official AV2 conformance vectors. libavm-diff is used during development but is not gating.

### M0 — Walking skeleton ✅ (complete with caveats)

**Goal.** Prove the end-to-end pipeline exists before building tools into it.

**Scope.** Single tile, keyframe only, 8-bit 4:2:0, 64×64 SB, recursive SPLIT partition down to 4×4 (no HORZ / VERT / etc.), DC intra only, single transform size (4×4 DCT_DCT), no post-filters, no film grain, no segmentation, fixed QP. Entropy coder + CDF init + coeff reader at the minimum viable surface. Runs through `decoder/core.rs` and writes into a `frame_buffer::FrameBuffer`, returned via the existing `avm_image_t` shim.

**Exit criterion.** A hand-crafted synthetic stream (committed to the test corpus) round-trips through the pure-Rust path bit-exact against libavm on the same stream.

**Status as of 2026-04-15.** All exit criteria met. The following M0 caveats are carried forward into M1 as the starting backlog: non-zero coefficient path is still `UnimplementedInM0`; CDF tables are uniform placeholders; partition walker is SPLIT-only; only DC intra; only DCT4×4; only single-tile; only 8-bit. The frame header parser now covers the full KF path (quant, loop filter, delta-Q, qmatrix, tx_mode) beyond the original M0 spec scope — this was done to fix a correctness bug (review finding #3) rather than as a scope increase.

### M1 — Full intra toolset (keyframe-only conformance)

**Goal.** Pass every KF-only conformance vector.

**Scope.** Finish full uncompressed frame header (`bitstream.rs`) — remaining fields: order_hint, superres, segmentation, delta_lf, CDEF params, loop restoration params, film grain params. Multi-tile within a frame (tile group OBU extension). Full partition tree (all HORZ / VERT / compound types, 128×128 SBs). All intra modes (directional + edge filter + upsample, Paeth, SMOOTH*, CFL, recursive filters). All transform sizes + types. Full coefficient decode (EOB / base / br / sign, neighbor context). Full dequant + QM application. Segmentation. Delta Q. Skip. Replace uniform CDF placeholders with spec-derived values; add CDF adaptation and per-tile CDF reset. Xiph AOMCTC smoke test (Task 0.3 from M1 plan) as a CI gate. Still scalar, still single-threaded, still no post-filters.

**Exit criterion.** All KF-only vectors pass. Palette + IBC may slip to M1.5 per risk R3.

### M2 — Inter (single-reference P-frame conformance)

**Goal.** Pass single-ref inter conformance vectors.

**Scope.** `inter/refs.rs` (DPB, refresh_frame_flags, order-hint-aware ref lists), `inter/mv.rs` (MV stack, dynamic ref list, MVP selection for single-ref modes only — NEAR, NEAREST, NEW, GLOBAL), `inter/mc.rs` (subpel MC filters for all four filter sets, luma + chroma). Still no compound, no OBMC, no warped.

**Exit criterion.** Single-ref P-frame vectors pass.

### M3 — Advanced inter (compound, warped, OBMC, global)

**Goal.** Pass remaining inter conformance vectors.

**Scope.** Compound modes (AVG, DIST, WEDGE, DIFFWTD, interintra), warped motion (per-block + global), OBMC, compound ref frames, skip_mode, full ref_mv context. Most MV-derivation bugs surface here — expect heavy use of the differential harness as a dev tool.

**Exit criterion.** All inter conformance vectors pass (KF + P + B + compound + warped).

### M4 — Post-filters, film grain, bit-depth, chroma formats

**Goal.** Pass all Main-profile conformance vectors at the scalar tier.

**Scope.** `loopfilter.rs`, `cdef.rs`, `restoration.rs`, `film_grain.rs`. 10-bit 4:2:0 throughout — touches every kernel, planned here rather than retrofitted. 4:2:2 / 4:4:4 / 12-bit only if conformance vectors require them; otherwise post-M7. Still-picture mode, showable-frame ordering corner cases, multi-layer as required.

**Exit criterion.** 100% of the Main-profile conformance vectors pass in the pure-Rust backend, scalar + single-threaded. **This is the correctness finish line.** M5–M7 add no decode capability.

### M5 — SIMD kernels

**Goal.** Close most of the perf gap to libavm.

**Scope.** Fill in `kernels/simd_x86.rs` (AVX2 primary, AVX-512 where it helps) and `kernels/simd_aarch64.rs` (NEON). Priority order from profiling, expected hot set: inverse transforms, subpel MC, deblocking, CDEF filter, LR (Wiener + self-guided), directional intra. Runtime CPU-feature dispatch via the `Kernels` trait — no duplication of the decode state machine. `unsafe` permitted inside these modules per §4.3.

**Exit criterion.** Conformance still 100% (re-run the whole suite after SIMD lands — catching lane-boundary bugs is the entire point). Perf: within ~25% of libavm single-threaded on the representative corpus. Closing the remaining gap is M6's job.

### M6 — Tile-parallel executor

**Goal.** Hit the ~10% perf target.

**Scope.** Real parallel `TileExecutor` implementation (rayon or a small custom thread pool — decide during M6 based on overhead measurements; §5.2 open question 4). Ref-frame lifetimes made explicit so they survive parallel tile completion. No frame pipelining yet (that stays deferred). `threads` field on `DecoderBuilder` finally does something in the Rust backend.

**Exit criterion.** Conformance still 100% under `--test-threads=1` and with the threaded executor. Perf within ~10% of libavm on the representative corpus at matching thread counts. TSan clean.

### M7 — Conformance sweep, FFI removal, cleanup

**Goal.** Retire libavm as a backend.

**Scope.** Full conformance sweep including any vectors deferred earlier. Delete `backend/libavm.rs` and the `Libavm` enum variant; the `sys` FFI module and `avm_image_t` interop stay (they're the output ABI). Keep `diff.rs` behind `#[cfg(feature = "libavm-diff")]` so future regressions can be triaged. Update `BackendKind` default, update docs, remove the dev dependency on the C build.

**Exit criterion.** `BackendKind::Rust` is the default and only backend. All conformance vectors pass. Perf target met. Crate builds without the libavm C library on the host.

### Summary

```
M0  walking skeleton            → synthetic stream bit-exact             ✅ done
M1  full intra toolset          → KF-only conformance vectors            🔄 in progress
M2  single-ref inter            → single-ref P-frame vectors
M3  advanced inter              → all inter conformance vectors
M4  post-filters + 10-bit       → 100% Main-profile conformance (CORRECTNESS DONE)
M5  SIMD kernels                → conformance held, ≤25% gap to libavm
M6  tile-parallel executor      → conformance held, ≤10% gap to libavm (PERF DONE)
M7  FFI removal, cleanup        → Rust is the only backend
```

---

## 4. Testing, performance, and `unsafe` policy

### 4.1 Testing strategy

**Primary oracle: official AV2 conformance vectors.** Each milestone exits on a named subset. The conformance suite lives outside the repo (too large to commit) — fetch script + cached hashes under `tests/conformance/`. CI runs a smoke subset on every push; the full suite runs nightly and on release candidates. The user is personally sourcing the official vectors (§5.2 Q2).

**Secondary oracle: libavm differential harness.** `src/diff.rs` + `tests/differential_test.rs` stay as a dev tool throughout M0–M6. Purpose: when a conformance vector fails, diff against libavm on the same input to localize the first frame, tile, SB, and partition where the two decoders disagree. Not gating, not in normal CI — on-demand during debugging and in the nightly job.

**Smoke test: Xiph AOMCTC corpus.** 30 single-frame YUV stills from the AOM CTC `f2_still_MidRes` set, encoded with the reference encoder, cached at `tests/smoke/cache/` (gitignored). Exercises the Rust backend on real-world encoded data and checks for crashes and byte-exact libavm parity. Added in M1 Task 0.3. Not a substitute for conformance vectors.

**Unit tests per subsystem.** Every module in `decoder/` ships with table-driven unit tests against known-answer vectors (transforms, BAC, MC filter taps, CDEF filter, Wiener). These are the only sane way to test SIMD kernels against their scalar reference in M5.

**Fuzzing.** The existing `fuzz/` crate targets the FFI decoder. Add pure-Rust fuzz targets for (1) OBU parser, (2) uncompressed header parser, (3) the full decoder front-door. Catching panics and UB in safe code is the backstop against `unsafe` in kernels.

**Sanitizers.** `cargo +nightly miri test` on the scalar core in CI (Miri can't run SIMD intrinsics; kernels excluded). `RUSTFLAGS=-Zsanitizer=address` and `-Zsanitizer=thread` runs nightly, especially M6 for the threaded executor. Note: Miri is currently blocked by local toolchain setup (`cargo-miri` unavailable) — tracked as an open M0 caveat.

### 4.2 Performance strategy

**The `Kernels` trait is load-bearing.** Every hot kernel ships in two forms from M0 onward: a scalar impl in `kernels/scalar.rs` and a trait method on `Kernels`. SIMD impls in M5 are drop-in replacements — no caller changes. This makes M5 a concentrated, isolated workstream.

**Runtime dispatch, not `#[cfg]`.** A `Kernels::detect() -> &'static dyn Kernels` returns the best impl available on the current CPU. `cfg` gates the *existence* of SIMD modules per target arch; runtime detection picks which one at startup.

**Layout decisions made in M0 that matter later.**

- Frame buffers are plane-major with per-plane stride; stride is a multiple of 64 bytes so SIMD loads are always aligned. No AoS pixel formats internally — convert only at the output boundary.
- Transform coefficient buffers are stack-allocated where the size is bounded (≤32×32) and arena-allocated for 64×64. No per-block heap allocation.
- The partition walk is iterative with an explicit stack, not recursive. Bounded stack pressure, SIMD-friendly inner loops.
- `Kernels` trait methods take `&mut [i16]` / `&[u8]` slices, not pointers. SIMD impls use `unsafe` internally to drop bounds checks; the trait boundary stays safe.
- Every kernel and buffer is generic over a `Pixel` type (`u8` or `u16`) from day one. Protects against the 10-bit retrofit pain in M4 — see risk R4.

**`TileExecutor` seam in M0.** Single-threaded default is an impl that calls `Fn` directly. M6's threaded impl is a second struct implementing the same trait. No state-machine changes in M6 — only lifetime adjustments on the ref-frame manager.

### 4.3 `unsafe` policy

Concretely:

- **Crate root:** no `#![forbid(unsafe_code)]` (kernels need it), but `#![deny(unsafe_op_in_unsafe_fn)]` is mandatory.
- **Every `unsafe` block** requires a `// SAFETY:` comment explaining the invariant. Enforced by `clippy::undocumented_unsafe_blocks` in CI. *Note: `src/backend/libavm.rs` currently violates this — all unsafe blocks are missing `SAFETY:` comments. This is tracked as a Critical finding in the M0 code review.*
- **Allowed to use `unsafe`:** `decoder/kernels/simd_*.rs`, `backend/rust.rs` (FFI output interop — `avm_image_t`, `avm_codec_frame_buffer_t`), `decoder/frame_buffer.rs` only for frame-buffer-manager raw-pointer interop, and narrowly-justified bounds-check elisions in `kernels/scalar.rs` behind a tested helper.
- **Forbidden to use `unsafe`:** `decoder/core.rs`, `entropy.rs`, `symbols.rs`, `partition.rs`, `intra.rs`, `inter/*`, `transform.rs` (the table-driven outer layer — kernels live in `kernels/`), `quant.rs`, `loopfilter.rs`, `cdef.rs`, `restoration.rs`, `film_grain.rs`, `executor.rs`, `threaded.rs`. These enforce the ban with per-module `#![forbid(unsafe_code)]`. *Note: `frame_buffer.rs` is currently missing this attribute — tracked as an open suggestion in the M0 code review.*
- **Miri in CI** on everything except `kernels/simd_*.rs`. The forbidden-`unsafe` modules guarantee Miri can actually run them.

The split: `unsafe` is concentrated in ~5% of the code (SIMD kernels + FFI) and the other 95% — including every bug-prone state-machine module — is safe Rust that Miri can verify.

---

## 5. Risks, open questions, and migration notes

### 5.1 Risks

**R1 — CDF tables and symbol coverage drift.** AV2 has hundreds of CDFs and the spec changes between drafts. The mechanical port is cheap; verification that each table matches the current spec is not. **Mitigation:** land a code-generator (Python or `build.rs`) that ingests the spec's machine-readable tables if one exists; otherwise hand-port once and guard with a unit test that hashes every table. Any mismatch with libavm during differential testing immediately points here.

**R2 — MV derivation is where bugs live.** The ref_mv stack, dynamic ref list, and MVP selection have more conditional branches per line than anywhere else in the decoder, and a single-bit error propagates silently through subsequent frames. **Mitigation:** M3's exit gate is not just conformance but a clean differential run against libavm — MV bugs hide from vectors that happen not to exercise the wrong path. Expect to spend more time here than the milestone sizing suggests.

**R3 — Palette + IBC depth.** Called out in M1 scope as "may slip to M1.5." If screen-content conformance vectors force a full palette + IBC implementation, that's a multi-week subsystem (color cache, palette token decode, IBC MV constraints). **Mitigation:** if it slips, it slips to M1.5 — don't let it block the KF-only milestone for the rest of the intra toolset.

**R4 — 10-bit retrofits.** 10-bit is scheduled for M4 as a single cross-cutting pass. If any M1–M3 kernel was written `u8`-only, M4 becomes a rewrite. **Mitigation:** every kernel and buffer in M0 is generic over the `Pixel` type from day one; the scalar impl just monomorphizes to `u8` until M4 adds the `u16` instantiation. Review-checklist item for every PR in M0–M3.

**R5 — SIMD doesn't get us to 10%.** M5 might land at a 20–25% gap. **Mitigation:** accept it; M6 threading covers the rest on multi-core. If M6 still doesn't close the gap, the followup is frame pipelining (the deferred option from the design conversation), not an expansion of M5 scope.

**R6 — Conformance vectors disagree with libavm.** When this happens (and it will), the spec wins per §1.1. But the differential harness becomes noise for that specific vector. **Mitigation:** a small `conformance_overrides.toml` listing vectors where libavm is known non-conformant, consulted by `diff.rs` so the harness doesn't flag them as regressions.

**R7 — Threading correctness under TSan.** M6's threaded executor is the first time ref-frame lifetimes are exposed to concurrent readers. **Mitigation:** `inter/refs.rs` in M2 commits to ref frames as `Arc<Frame>`-style handles even in the single-threaded case, so M6 inherits the right ownership model.

**R8 — Fuzzer finds panics in safe code.** This is the goal, not a risk — but expect a backlog of panic fixes in M4–M5. Budget time.

### 5.2 Open questions

1. **Spec version to target.** Which AV2 draft revision is the source of truth? Pin it in the doc and in `bitstream.rs` so CDFs and syntax match. *Still open.*

2. **Conformance vector source.** Where do the official vectors live, how are they fetched, what's their license? *Actively being worked — the user is personally sourcing them. `tests/conformance/` scaffolding exists and is ready. The Xiph AOMCTC corpus (`tests/smoke/cache/`) serves as a smoke-test-only stand-in until official vectors arrive.*

3. **Corpus for perf measurement.** The ~10% target is meaningless without a named corpus. Recommend: pin 4–6 streams (mix of animation, film, screen content; 1080p + 4K; 8-bit + 10-bit) and commit their hashes. *Still open.*

4. **Thread-pool choice.** Rayon vs custom pool. Decide during M6 based on measured per-tile overhead — flagged as deferred on purpose. *Still open.*

5. **Kernels crate split.** Does `decoder/kernels/` eventually become its own crate so it can be reused (or no-std'd) later? Not needed for M0–M7, but affects the module boundary design slightly. Recommend: keep it as a module for now, design the trait as if it were a crate boundary. *Still open.*

6. **Code-generator for CDF tables.** Does the AV2 spec ship machine-readable CDF data, or is it prose-only? Affects R1 mitigation cost. *Still open.*

### 5.3 Migration notes

- **Public API is untouched.** `Decoder`, `DecoderBuilder`, `FrameBufferManager`, `DecoderError`, `BackendKind` — none change shape. M7's removal of `BackendKind::Libavm` is the one breaking change, and it's a single enum variant.
- **`avmdec` CLI** follows whichever backend is the default. M0–M6: libavm default, opt into `--backend rust`. M7: Rust default, `--backend libavm` disappears.
- **Build-system impact.** The crate currently vendors / links libavm via `build.rs`. That stays until M7. Post-M7, the C dependency is gone — the crate becomes a pure Rust build, which is a significant downstream win for packaging, cross-compilation, and CI time.
- **`diff.rs` lifecycle.** Dev tool throughout M0–M6. At M7, move behind `#[cfg(feature = "libavm-diff")]` so future conformance regressions can be triaged against libavm from a git checkout without restructuring.
