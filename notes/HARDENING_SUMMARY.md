# Hardening Summary: rustavm FFI Safety

**Date:** 2026-04-07
**Scope:** `rustavm/src/decoder.rs`, `rustavm/src/bin/avmdec.rs`, `rustavm/tests/`

---

## 1. Unsafe Patterns Eliminated

### Critical

| ID | Issue | Fix |
|----|-------|-----|
| C-1 | `std::mem::transmute` on frame-buffer callback function pointers — suppressed type checking, any signature drift would silently produce UB | Removed. `set_frame_buffer_functions` now declares callback parameters with `c_int` return types that exactly match the bindgen-generated `avm_get_frame_buffer_cb_fn_t` / `avm_release_frame_buffer_cb_fn_t`. The function is marked `unsafe` with documented safety requirements. |
| C-2 | `slice::from_raw_parts` with negative stride — casting negative `int` to `usize` produced astronomically large slice lengths | `plane()` and `stride()` check `stride < 0` and return `None` / `0` respectively. Negative strides (from `avm_img_flip`) are explicitly rejected with documentation. |

### High

| ID | Issue | Fix |
|----|-------|-----|
| H-1 | `stride * height` overflow in `plane()` — unchecked `usize` multiplication could wrap | Uses `stride.checked_mul(height)?`; returns `None` on overflow. |
| H-3 | `String::leak()` in `avmdec.rs` — deliberate memory leak for lifetime workaround | Replaced with `Cow<'static, str>` — no heap leak, proper ownership. |

## 2. Validation Added

| Guard | Location | Purpose |
|-------|----------|---------|
| `index >= 3` bounds check | `plane()`, `stride()`, `plane_width()`, `chroma_plane_height()` | Prevents out-of-bounds access into C arrays of length 3 |
| `stride < plane_width` | `plane()` | Rejects corrupted frames where stride is narrower than active pixel data |
| `len > img.sz` when `sz > 0` | `plane()` | Prevents constructing a slice that exceeds the total image allocation |
| `x/y_chroma_shift.min(31)` | `plane_width()`, `chroma_plane_height()` | Prevents UB from shifting `usize` by >= bit width |
| `debug_assert!(!self.img.is_null())` | All 11 `Frame` methods | Catches null pointer dereference in debug builds |

## 3. Documentation Added

| Topic | Location |
|-------|----------|
| `decode()` data lifetime — confirmed synchronous, no pointer retention | `decoder.rs:82-90` |
| `get_frames()` double-call safety — borrow checker prevents concurrent iterators | `decoder.rs:108-115` |
| `set_frame_buffer_functions` safety contract — `priv_` lifetime, panic UB | `decoder.rs:148-154` |
| `Drop` + FFI panic interaction — UB risk documented | `decoder.rs:209-213` |

## 4. Test Coverage

### Summary

| Suite | Total | Pass | Ignored | Fail |
|-------|-------|------|---------|------|
| Unit tests (`lib.rs`) | 24 | 24 | 0 | 0 |
| Frame buffer (`frame_buffer_test.rs`) | 23 | 13 | 10 | 0 |
| MD5 verification (`md5_verification_test.rs`) | 12 | 2 | 10 | 0 |
| **Total** | **59** | **39** | **20** | **0** |

All 20 ignored tests are gated on `LIBAVM_TEST_DATA_PATH` and are structurally complete.

### Categories Covered

**Frame buffer tests (no data required):**
- Callback registration and type safety (compile-time transmute regression test)
- Null callback rejection via raw FFI (safe wrapper prevents this at compile time)
- Manager lifecycle: counters, pool exhaustion, double release, buffer reuse/growth
- Drop ordering: decoder-before-manager and manager-before-decoder
- Failure injection: NullData, OneLessByte, NoRelease modes

**Frame buffer tests (data required):**
- Full decode with external buffers, callback invocation verification
- Jitter buffers, minimum buffers, insufficient buffers
- Null allocation, undersized allocation, no-release error paths
- Late registration (set after decode), buffer data accessibility

**MD5 verification tests (data required):**
- 8-bit: quantizer-00, quantizer-63, all-64 sweep
- 10-bit: quantizer-00, all-64 sweep (HIGHBITDEPTH 2-bytes/sample path)
- Frame sizes: 16x16, non-power-of-two dimensions
- Feature vectors: all-intra, CDF-update, film grain, SVC
- Multi-threaded consistency: 1/2/4 threads produce identical MD5s (8-bit and 10-bit)

### Code Quality

- **Clippy:** Zero warnings
- **Build:** Debug and release both clean, zero warnings
- **Doc build:** All warnings are from auto-generated `bindings.rs` (Doxygen `\param[in]` syntax); zero warnings from hand-written code

## 5. Public API Changes

| Change | Justification |
|--------|---------------|
| `set_frame_buffer_functions` callback return type: `i32` -> `c_int` | Soundness: eliminates transmute. `c_int` is `i32` on all current targets but matches the C typedef exactly. |
| `plane()` returns `None` in more cases (negative stride, stride < width, overflow, sz bound) | Soundness: previously returned unsound slices in these cases. |
| `stride()` returns `0` for negative strides | Soundness: previously returned a huge `usize` from negative-to-unsigned cast. |
| `plane_width()` / `chroma_plane_height()` return `0` for index >= 3 | Correctness: previously returned meaningless chroma-shifted values. |
| New public trait `FrameBufferManager` | Addition only; no existing code affected. |

## 6. Remaining Risks

### Not addressed (out of scope for `decoder.rs` hardening)

| ID | Severity | Issue | File | Mitigation path |
|----|----------|-------|------|-----------------|
| H-2 | High | IVF frame size uncapped (up to 4 GiB allocation) | `ivf.rs` | Add configurable max frame size |
| H-4 | High | `priv_` pointer has no Rust lifetime tie to `Decoder` | `decoder.rs` | Wire up `ext_fb_manager` field |
| M-1 | Medium | FFI callbacks lack `catch_unwind`; panic through C is UB | `decoder.rs` | Add safe callback wrapper with `catch_unwind` |
| M-3 | Medium | Thread safety (`Send`/`Sync`) not documented | `decoder.rs` | Add doc comments on `Decoder` |
| M-4 | Medium | `avmdec.rs` plane loops use direct indexing; corrupted frames panic | `avmdec.rs` | Use `.get()` with error handling |
| L-1 | Low | `std::mem::zeroed()` fragile for future struct changes | `decoder.rs` | Document zero-init contract |
| L-2 | Low | `*const` vs `*mut` for `avm_codec_get_frame` return | `decoder.rs` | Store as `*mut`, expose read-only |
| L-3 | Low | Hardcoded GCC include path in `build.rs` | `build.rs` | Use `cc`/`pkg-config` discovery |

## 7. Recommendations for Future Work

1. **Fuzzing:** Run `cargo-fuzz` against `Decoder::decode()` with arbitrary byte inputs. The hardened `plane()` should now reject malformed frames gracefully, but fuzzing will validate this and find any remaining edge cases in the C library.

2. **MIRI testing:** Run the unit tests under MIRI (`cargo +nightly miri test`) to detect undefined behavior in the Rust-side unsafe code. MIRI cannot cross the FFI boundary, but it will validate the pure-Rust unsafe patterns (slice construction, pointer arithmetic).

3. **Test vectors:** Obtain the full AV2 conformance test data set and run the 20 ignored tests. This validates the MD5 correctness path end-to-end and exercises the frame buffer lifecycle under real decode workloads.

4. **IVF frame size cap (H-2):** Add a `max_frame_size` parameter to `IvfReader` to prevent DoS from crafted files. A default of 256 MiB is reasonable.

5. **Safe callback wrapper (H-4 + M-1):** Complete the `ext_fb_manager` integration: accept `Box<dyn FrameBufferManager>`, store it in `Decoder`, pass a pointer to the boxed value as `priv_`, and wrap the callback shim in `catch_unwind`.

6. **Address sanitizer:** Run the test suite under ASan (`RUSTFLAGS="-Zsanitizer=address"`) to detect memory errors in the C library triggered by the Rust test harness.
