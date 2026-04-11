# rustavm: pure-Rust AV2 decoder — design

**Date:** 2026-04-11
**Audience:** collaborators onboarding onto the pure-Rust decoder workstream. Assumes Rust fluency but not deep AV2 internals.
**Status:** design, pre-implementation.
**How to use this doc.** This is the end-to-end design for the entire M0–M7 conversion. It is deliberately too large to execute as a single implementation plan. Each milestone (M0, M1, …) gets its own implementation plan written against this spec, in order. Start with M0.

## 0. AV2 primer for collaborators

A one-screen crash course so the rest of this doc is legible. Skip if you already decode video for a living.

- **OBU (Open Bitstream Unit).** AV2 streams are a sequence of OBUs: sequence header, temporal delimiter, frame header, tile group, metadata, etc. Each OBU has a 1-byte header (plus optional extension byte) and an optional size field. `src/bitstream.rs` already parses this layer.
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
  bitstream.rs                  # existing OBU + headers; grows to full uncompressed header
  decoder.rs                    # existing public API surface; unchanged
  decoder/
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

### 1.3 Starting state

As of 2026-04-11 (`git rev-parse HEAD` → `082f9682dc`, plus uncommitted work in `src/backend/`, `src/bitstream.rs`, `src/diff.rs`, `tests/differential_test.rs`):

- `backend::Libavm` wraps the C decoder and is the default/only functional backend.
- `backend::Rust` exists as a scaffold. It parses OBUs, extracts a minimal sequence header, extracts a stub frame header, tracks decode events, and returns `Unimplemented` on any actual frame data.
- `src/bitstream.rs` (~785 LOC) handles OBU framing, a minimal sequence header, a stub `FrameHeaderInfo`, and frame-packet classification. It is the only meaningful pure-Rust decoder code in the repo today.
- `src/diff.rs` + `tests/differential_test.rs` provide a bit-exact differential harness that compares the Rust backend to libavm on IVF files.

Everything past bitstream parsing — entropy, partition, prediction, transform, post-filters, DPB — is 100% C via libavm.

## 2. Subsystem catalog

Every piece the pure-Rust decoder needs, grouped by layer. "Status" reflects the repo as of 2026-04-11; "owner" is the target module from §1.2.

### 2.1 Bitstream layer

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| OBU framing (split, header, extension, size) | ✅ done | `bitstream.rs` | Already landed. Review for conformance-corner cases: `obu_has_size_field=0` streams, temporal delimiter handling. |
| Sequence header | ⚠ minimal | `bitstream.rs` | Current parse stops at `max_frame_{width,height}`. Needs full: color_config, bit_depth, subsampling, operating points, timing info, decoder model info, initial_display_delay, film_grain_params_present, etc. |
| Uncompressed frame header | ⚠ stub | `bitstream.rs` | Current `FrameHeaderInfo` is a placeholder. Needs: frame_type, show_frame / showable_frame, error_resilient_mode, disable_cdf_update, allow_screen_content_tools, order_hint, primary_ref_frame, refresh_frame_flags, ref_frame_idx, ref_order_hint, render_size, superres, loop_filter_params, quant_params, segmentation, delta_q / delta_lf, cdef_params, lr_params, tx_mode, skip_mode, warped_motion, global_motion, film_grain_params. |
| Tile group OBU | ❌ missing | `bitstream.rs` | `tile_start` / `tile_end`, tile_size fields, per-tile bit offsets into the entropy-coded payload. |

### 2.2 Entropy + symbols

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Boolean arithmetic coder | ❌ missing | `entropy.rs` | AV2's BAC variant (AV1-family descendant). Straightforward port from spec; hot path for SIMD later (range / value update). |
| CDF tables + init / update | ❌ missing | `symbols.rs` | Hundreds of tables, adaptation rates, per-tile CDF reset, `disable_cdf_update` gating. Generate from spec if tooling exists — see risk R1. |
| Symbol readers (per-syntax-element) | ❌ missing | `entropy.rs` | Thin wrappers around BAC + CDF: partition, intra_mode, tx_type, coeff, mv, etc. One reader per spec syntax element. |

### 2.3 Block decode

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Superblock / partition tree | ❌ missing | `partition.rs` | Recursive partition decode (128×128 or 64×64 SBs), NONE / SPLIT / HORZ / VERT / HORZ_A / HORZ_B / VERT_A / VERT_B / HORZ_4 / VERT_4 plus AV2 additions. |
| Block info (mode_info) propagation | ❌ missing | `partition.rs` | Neighbor context (above / left) for mode / MV prediction. |
| Segmentation | ❌ missing | `partition.rs` | Per-block seg_id, feature deltas (Q, LF, ref, skip). |
| Skip / skip_mode | ❌ missing | `partition.rs` | |
| Palette + intra-block-copy | ❌ missing | `intra.rs` | Screen-content tools. Required by some conformance vectors — see risk R3. |

### 2.4 Coefficients & reconstruction

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Coefficient decoding | ❌ missing | `entropy.rs` + `transform.rs` | EOB, base / br levels, sign, DC sign context, coeff_context derivation. Complex neighbor-dependent context math. |
| Dequantization + QM | ❌ missing | `quant.rs` | AC / DC quant tables, quantization matrices, per-plane, per-segment. |
| Inverse transforms | ❌ missing | `transform.rs` + `kernels/` | DCT / ADST / FLIPADST / IDTX / WHT combos, 4×4 through 64×64, rectangular sizes. Hot kernel — behind `Kernels` trait from day one so SIMD drops in in M5. |

### 2.5 Prediction

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Intra modes | ❌ missing | `intra.rs` | DC, SMOOTH / SMOOTH_H / SMOOTH_V, directional (+ interpolation filter), Paeth, chroma-from-luma, recursive intra filters, palette. |
| Intra edge filter + upsample | ❌ missing | `intra.rs` | Per-direction pre-filter of reference samples. |
| MV derivation | ❌ missing | `inter/mv.rs` | Spatial + temporal MV stack, dynamic ref list, MVP selection, NEW / NEAR / GLOBAL / NEAR_NEW compound modes. Most error-prone inter subsystem — see risk R2. |
| Reference frame manager (DPB) | ❌ missing | `inter/refs.rs` | 8 ref slots, refresh_frame_flags bookkeeping, ref_order_hint, order-hint-aware ref list construction. |
| Subpel MC filters | ❌ missing | `inter/mc.rs` + `kernels/` | 8-tap filters (REGULAR / SMOOTH / SHARP / BILINEAR), luma + chroma, 1/16 pel. Hot kernel. |
| Compound prediction | ❌ missing | `inter/mc.rs` | AVG, DIST, WEDGE, DIFFWTD, interintra, compound masks. |
| Warped motion + global motion | ❌ missing | `inter/mc.rs` + `kernels/` | Affine warp per block / global, 8-tap warped filter. |
| OBMC | ❌ missing | `inter/mc.rs` | Overlap blending with neighbor MVs. |

### 2.6 Post-filters

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Deblocking | ❌ missing | `loopfilter.rs` + `kernels/` | Per-edge strength derivation, 4 / 6 / 8 / 14-tap filters. Hot kernel. |
| CDEF | ❌ missing | `cdef.rs` + `kernels/` | Primary / secondary strengths, directional search, per-SB. Hot kernel. |
| Loop restoration | ❌ missing | `restoration.rs` + `kernels/` | Wiener and self-guided restoration units. Hot kernel. |
| Film grain synthesis | ❌ missing | `film_grain.rs` | AR grain generation + scaling LUTs, applied to output image. |

### 2.7 Frame lifecycle

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| Reconstructed frame storage | ❌ missing | `frame_buffer.rs` | Planar Y / U / V with stride, bit-depth-generic storage. Backed by `FrameBufferManager` so external allocators still work. |
| Output queue | ⚠ partial | `decoder/core.rs` | `show_frame` / `show_existing_frame` / `showable_frame` ordering. `tests/differential_test.rs` already exercises this against libavm. |
| Error model | ⚠ partial | `decoder.rs` | `DecoderError` exists; needs new variants for parse vs conformance vs unimplemented-tool vs buffer-manager errors. |

### 2.8 Perf & concurrency seams

| Subsystem | Status | Owner | Notes |
|---|---|---|---|
| `Kernels` trait | ❌ missing | `decoder/kernels/mod.rs` | Scalar default; SIMD impls selected at runtime via CPU feature detection. Covers: idct / iadst (all sizes), subpel MC, deblock, CDEF, LR Wiener, LR self-guided, directional intra. |
| `TileExecutor` trait | ❌ missing | `decoder/executor.rs` | Single-threaded default; parallel impl in M6. |
| FFI compat layer | ⚠ partial | `backend/rust.rs` | Must produce `avm_image_t` output so the existing public API keeps working unchanged. |

## 3. Milestone plan

Each milestone exits on passing a curated subset of official AV2 conformance vectors. libavm-diff is used during development but is not gating.

### M0 — Walking skeleton

**Goal.** Prove the end-to-end pipeline exists before building tools into it.

**Scope.** Single tile, keyframe only, 8-bit 4:2:0, 64×64 SB, recursive SPLIT partition down to 4×4 (no HORZ / VERT / etc.), DC intra only, single transform size (4×4 DCT_DCT), no post-filters, no film grain, no segmentation, fixed QP. Entropy coder + CDF init + coeff reader at the minimum viable surface. Runs through `decoder/core.rs` and writes into a `frame_buffer::FrameBuffer`, returned via the existing `avm_image_t` shim.

Note: the 4×4-leaf choice is deliberate — it keeps block size and transform size aligned so there is no sub-block transform-partitioning in M0, while still exercising the recursive partition walk, which is the machinery M1 needs to expand.

**New code.** `entropy.rs` (BAC core), `symbols.rs` (tables touched by the above path only), `partition.rs` (skeleton with SPLIT-only recursion), `transform.rs` (DCT4×4 only, scalar), `quant.rs` (fixed dequant), `intra.rs` (DC only), `frame_buffer.rs`, `decoder/core.rs` skeleton driver, `kernels/mod.rs` + `scalar.rs` stubs, `executor.rs` single-threaded default.

**Exit criterion.** A hand-crafted synthetic stream (committed to the test corpus) round-trips through the pure-Rust path bit-exact against libavm on the same stream. libavm is used as a dev-time oracle here because no official conformance vector exercises this specific sub-profile; the real conformance gate starts at M1.

### M1 — Full intra toolset (keyframe-only conformance)

**Goal.** Pass every KF-only conformance vector.

**Scope.** Finish full uncompressed frame header (`bitstream.rs`), tile group OBU + multi-tile within a frame, full partition tree, all intra modes (directional + edge filter + upsample, Paeth, SMOOTH*, CFL, recursive filters), all transform sizes + types, full coefficient decode (EOB / base / br / sign, neighbor context), full dequant + QM, segmentation, delta Q, skip. Still scalar, still single-threaded, still no post-filters.

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
M0  walking skeleton            → synthetic stream bit-exact
M1  full intra toolset          → KF-only conformance vectors
M2  single-ref inter            → single-ref P-frame vectors
M3  advanced inter              → all inter conformance vectors
M4  post-filters + 10-bit       → 100% Main-profile conformance (CORRECTNESS DONE)
M5  SIMD kernels                → conformance held, ≤25% gap to libavm
M6  tile-parallel executor      → conformance held, ≤10% gap to libavm (PERF DONE)
M7  FFI removal, cleanup        → Rust is the only backend
```

## 4. Testing, performance, and `unsafe` policy

### 4.1 Testing strategy

**Primary oracle: official AV2 conformance vectors.** Each milestone exits on a named subset. The conformance suite lives outside the repo (too large to commit) — fetch script + cached hashes under `tests/conformance/`. CI runs a smoke subset on every push; the full suite runs nightly and on release candidates.

**Secondary oracle: libavm differential harness.** `src/diff.rs` + `tests/differential_test.rs` stay as a dev tool throughout M0–M6. Purpose: when a conformance vector fails, diff against libavm on the same input to localize the first frame, tile, SB, and partition where the two decoders disagree. Not gating, not in normal CI — on-demand during debugging and in the nightly job.

**Unit tests per subsystem.** Every module in `decoder/` ships with table-driven unit tests against known-answer vectors (transforms, BAC, MC filter taps, CDEF filter, Wiener). These are the only sane way to test SIMD kernels against their scalar reference in M5.

**Fuzzing.** The existing `fuzz/` crate targets the FFI decoder. Add pure-Rust fuzz targets for (1) OBU parser, (2) uncompressed header parser, (3) the full decoder front-door. Catching panics and UB in safe code is the backstop against `unsafe` in kernels.

**Sanitizers.** `cargo +nightly miri test` on the scalar core in CI (Miri can't run SIMD intrinsics; kernels excluded). `RUSTFLAGS=-Zsanitizer=address` and `-Zsanitizer=thread` runs nightly, especially M6 for the threaded executor.

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
- **Every `unsafe` block** requires a `// SAFETY:` comment explaining the invariant. Enforced by `clippy::undocumented_unsafe_blocks` in CI.
- **Allowed to use `unsafe`:** `decoder/kernels/simd_*.rs`, `backend/rust.rs` (FFI output interop — `avm_image_t`, `avm_codec_frame_buffer_t`), `decoder/frame_buffer.rs` only for frame-buffer-manager raw-pointer interop, and narrowly-justified bounds-check elisions in `kernels/scalar.rs` behind a tested helper.
- **Forbidden to use `unsafe`:** `decoder/core.rs`, `entropy.rs`, `symbols.rs`, `partition.rs`, `intra.rs`, `inter/*`, `transform.rs` (the table-driven outer layer — kernels live in `kernels/`), `quant.rs`, `loopfilter.rs`, `cdef.rs`, `restoration.rs`, `film_grain.rs`, `executor.rs`, `threaded.rs`. These enforce the ban with per-module `#![forbid(unsafe_code)]`.
- **Miri in CI** on everything except `kernels/simd_*.rs`. The forbidden-`unsafe` modules guarantee Miri can actually run them.

The split: `unsafe` is concentrated in ~5% of the code (SIMD kernels + FFI) and the other 95% — including every bug-prone state-machine module — is safe Rust that Miri can verify.

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

### 5.2 Open questions (to resolve before M0 starts)

1. **Spec version to target.** Which AV2 draft revision is the source of truth? Pin it in the doc and in `bitstream.rs` so CDFs and syntax match.
2. **Conformance vector source.** Where do the official vectors live, how are they fetched, what's their license? Needs an answer before `tests/conformance/` can be designed.
3. **Corpus for perf measurement.** The ~10% target is meaningless without a named corpus. Recommend: pin 4–6 streams (mix of animation, film, screen content; 1080p + 4K; 8-bit + 10-bit) and commit their hashes.
4. **Thread-pool choice.** Rayon vs custom pool. Decide during M6 based on measured per-tile overhead — flagged as deferred on purpose.
5. **Kernels crate split.** Does `decoder/kernels/` eventually become its own crate so it can be reused (or no-std'd) later? Not needed for M0–M7, but affects the module boundary design slightly. Recommend: keep it as a module for now, design the trait as if it were a crate boundary.
6. **Code-generator for CDF tables.** Does the AV2 spec ship machine-readable CDF data, or is it prose-only? Affects R1 mitigation cost.

### 5.3 Migration notes

- **Public API is untouched.** `Decoder`, `DecoderBuilder`, `FrameBufferManager`, `DecoderError`, `BackendKind` — none change shape. M7's removal of `BackendKind::Libavm` is the one breaking change, and it's a single enum variant.
- **`avmdec` CLI** follows whichever backend is the default. M0–M6: libavm default, opt into `--backend rust`. M7: Rust default, `--backend libavm` disappears.
- **Build-system impact.** The crate currently vendors / links libavm via `build.rs`. That stays until M7. Post-M7, the C dependency is gone — the crate becomes a pure Rust build, which is a significant downstream win for packaging, cross-compilation, and CI time.
- **`diff.rs` lifecycle.** Dev tool throughout M0–M6. At M7, move behind `#[cfg(feature = "libavm-diff")]` so future conformance regressions can be triaged against libavm from a git checkout without restructuring.
