# AVM to Rust Porting Plan

## Overview
The goal is to extract the decoder part of the AVM project and build a Rust codec wrapper around it, providing a safe and idiomatic Rust interface.

## Current Status: COMPLETED (Core Tasks)

### Phase 0: Setup
- Cloned `avm` into `rustavm`.
- Identified key decoder headers: `avm/avm_decoder.h` and `avm/avmdx.h`.

### Phase 1: Rust Project & C Build Integration
- Created `Cargo.toml` with `cmake` and `bindgen`.
- Configured `build.rs` to build the AVM library with `CONFIG_AV2_ENCODER=0`.
- Successfully generated FFI bindings and linked against `libavm.a`.

### Phase 2: Initial Safe Wrapper
- Created `rustavm::decoder::Decoder` safe wrapper.
- Implemented `new()`, `decode()`, `get_frames()`, and `Drop`.
- Verified basic functionality with unit tests.

### Phase 4: Comprehensive API Coverage
- **External Frame Buffers**: Wrapped `avm_codec_set_frame_buffer_functions`.
- **Stream Information**: Wrapped `avm_codec_get_stream_info`.
- **Image Handling**: Safe `Frame` wrapper with plane/stride access and bit-depth handling.
- **Iterator Pattern**: `get_frames()` returns an `Iterator<Item = Frame>`.

### Phase 5: Rust CLI Application (`avmdec`)
- Implemented `avmdec` in Rust with IVF support (`src/bin/avmdec.rs`).
- Supports Y4M and raw YUV output.
- Supports MD5 calculation (matching C `avmdec` logic for high-bitdepth buffers).
- Successfully verified decoding on AV2 streams.

### Phase 6: Threading & Performance
- **Threading**: `Decoder::with_config(Some(threads))` implemented.
- **SIMD/AVX**: C-side optimizations are active in the build.

### Phase 7: Documentation & Examples
- Created `examples/simple_decode.rs`.
- Added MD5 support to verify correctness against C implementation.

### Phase 8: Security Audit & FFI Hardening
- **Security audit** performed on all `src/` files; findings in `SECURITY_AUDIT.md`.
- **C-1 (Critical):** Removed `transmute` in `set_frame_buffer_functions`; callback signatures now use `c_int` types matching bindgen directly.
- **C-2 (Critical):** `plane()` and `stride()` reject negative strides (vertically-flipped images).
- **H-1:** `stride * height` uses `checked_mul` to prevent overflow in `plane()`.
- **Additional guards in `plane()`:** stride vs. plane_width sanity check, `img.sz` upper-bound check.
- **M-2:** `plane_width()` and `chroma_plane_height()` now bounds-check `index >= 3` and clamp `x/y_chroma_shift` to 31.
- **H-3:** Removed `.leak()` in `avmdec.rs`; replaced with `Cow<'static, str>`.
- **Debug assertions:** All 11 `Frame` methods assert non-null `img` pointer.
- **Documentation:** `decode()` data lifetime, `get_frames()` double-call safety, and `Drop`/panic/FFI UB risk documented.

### Phase 9: Rust Integration Test Suite
- **`tests/frame_buffer_test.rs`:** 23 tests (13 always-run, 10 gated on `LIBAVM_TEST_DATA_PATH`). Covers callback registration, type safety, null-callback rejection, manager lifecycle, pool exhaustion, failure injection, and full decode-with-external-buffers.
- **`tests/md5_verification_test.rs`:** 12 tests (2 always-run, 10 gated). Covers MD5 infrastructure, 8-bit and 10-bit quantizer vectors, frame sizes, feature vectors, and multi-threaded decode consistency.

---

## Final Implementation Notes
- **MD5 Logic**: When bit-depth is 8 but the internal buffer is 16-bit (`AVM_IMG_FMT_HIGHBITDEPTH`), the MD5 calculation must truncate each 16-bit sample to 8-bit to match `avmdec`'s behavior. This is handled in `src/bin/avmdec.rs` and `tests/md5_verification_test.rs`.
- **Lifetimes**: `Frame` borrows from `Decoder` to ensure memory safety. The `FrameIterator` maintains the C iterator state correctly. The borrow checker statically prevents double-call and use-after-decode.
- **Callback Types**: `set_frame_buffer_functions` uses `c_int` types that exactly match the bindgen-generated `avm_get_frame_buffer_cb_fn_t` / `avm_release_frame_buffer_cb_fn_t` typedefs. No `transmute` is needed or used.
- **Build System**: The `build.rs` script is robust but assumes a Linux environment with GCC installed for `stddef.h`. For cross-platform support, consider using the `cc` crate's include path detection.

## Known Remaining Issues
- **H-2:** IVF frame size is uncapped (`ivf.rs`) — a crafted file can trigger up to 4 GiB allocation.
- **H-4:** `priv_` in `set_frame_buffer_functions` has no Rust lifetime tie to the `Decoder`. The `ext_fb_manager` field exists as a placeholder but is not yet wired up.
- **M-1:** FFI callbacks lack `catch_unwind`; panicking through C frames is UB.
- **M-3:** Thread safety (`Send`/`Sync`) not explicitly documented on `Decoder`.
- **M-4:** `avmdec.rs` plane loops use direct indexing; corrupted frames can panic.
- **L-1/L-2/L-3:** Minor issues (zeroed init, const mismatch, hardcoded include path) deferred.
