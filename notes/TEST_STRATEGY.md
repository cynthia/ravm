# Rust AV2 Decoder Wrapper — Test Strategy

## 1. C Test Infrastructure Summary

The C AV2 project (`avm/test/`) uses Google Test with the following key components:

| C Test File | What It Tests |
|---|---|
| `decode_api_test.cc` | NULL pointers, invalid params, double-init guards |
| `external_frame_buffer_test.cc` | External frame buffer lifecycle (alloc, reuse, release, error) |
| `test_vector_test.cc` | Per-frame MD5 verification against reference checksums |
| `test_vectors.cc` | 200+ IVF test vector file names (8-bit, 10-bit, various sizes, features) |
| `decode_test_driver.{h,cc}` | Decode loop infrastructure: init, decode, peek, frame iteration |
| `invalid_file_test.cc` | Corrupted/malformed IVF streams with expected error codes |
| `decode_multithreaded_test.cc` | Multi-thread decode correctness (compare single-thread vs N-thread MD5) |
| `md5_helper.h` | Per-plane MD5 computation over decoded `avm_image_t` |
| `ivf_video_source.h` | IVF file parsing and frame extraction for tests |
| `decode_perf_test.cc` | Decode performance benchmarks |

### Test Data Mechanism

- **Path resolution**: `LIBAVM_TEST_DATA_PATH` environment variable, or falls back to compile-time `LIBAVM_TEST_DATA_PATH` preprocessor define, or `.` (`avm/test/video_source.h:37-50`).
- **Download**: CMake downloads from `https://storage.googleapis.com/aom-test-data` using SHA1 checksums from `avm/test/test-data.sha1`.
- **MD5 reference files**: Named `<video>.ivf.md5`, contain one line per decoded frame: `<32-hex-md5>  img-<W>x<H>-<NNNN>.i420` (same format as `decode_to_md5.c` output).
- **Invalid file results**: Named `<video>.ivf.res`, contain one integer per frame indicating expected decode error code.
- **Local state**: Test vectors are NOT committed to the repo. They must be downloaded via CMake (`make testdata`) or manually. The build output directory is `avm/out/` but no test data was found there currently.

---

## 2. C Test Categories → Rust Test Mapping

### Category A: Decode API Validation

**C source**: `decode_api_test.cc` — `TEST(DecodeAPI, InvalidParams)`

Tests null/invalid arguments to all top-level codec functions. Maps directly to Rust because the safe wrapper must reject these at the Rust boundary (before they reach C).

| C Test Assertion | Rust Test |
|---|---|
| `avm_codec_dec_init(NULL, NULL, NULL, 0)` → `INVALID_PARAM` | `test_init_rejects_null_equivalent` (Rust wrapper should never allow this) |
| `avm_codec_dec_init(&dec, NULL, NULL, 0)` → `INVALID_PARAM` | N/A — Rust wrapper always provides iface |
| `avm_codec_decode(NULL, NULL, 0, NULL)` → `INVALID_PARAM` | `test_decode_empty_data` |
| `avm_codec_decode(&dec, NULL, sizeof(buf), NULL)` → `INVALID_PARAM` | `test_decode_null_data_nonzero_size` — should be impossible via safe API |
| `avm_codec_decode(&dec, buf, 0, NULL)` → `INVALID_PARAM` | `test_decode_zero_length` |
| `avm_codec_destroy(NULL)` → `INVALID_PARAM` | N/A — Rust Drop handles this |
| `avm_codec_error(NULL)` → non-NULL | N/A — error strings are internal |
| Valid init + decode with zero-length buf → `INVALID_PARAM` | `test_decode_zero_length_after_init` |
| Valid init + decode with null data → `INVALID_PARAM` | Prevented by Rust type system (`&[u8]` can't be null) |

### Category B: Frame Buffer Lifecycle

**C source**: `external_frame_buffer_test.cc`

| C Test | Rust Test | What It Verifies |
|---|---|---|
| `ExternalFrameBufferTest::MinFrameBuffers` | `test_ext_fb_minimum_buffers` | Decode succeeds with exactly `REF + WORK` buffers |
| `ExternalFrameBufferTest::EightJitterBuffers` | `test_ext_fb_with_jitter_buffers` | Extra buffers don't cause issues |
| `ExternalFrameBufferTest::NotEnoughBuffers` | `test_ext_fb_insufficient_buffers` | Returns `MEM_ERROR` when buffers exhausted |
| `ExternalFrameBufferTest::NoRelease` | `test_ext_fb_no_release_callback` | Buffers never returned → `MEM_ERROR` |
| `ExternalFrameBufferTest::NullRealloc` | `test_ext_fb_null_allocation` | Get callback returns NULL data → `MEM_ERROR` |
| `ExternalFrameBufferTest::ReallocOneLessByte` | `test_ext_fb_undersized_buffer` | Buffer too small → `MEM_ERROR` |
| `ExternalFrameBufferTest::NullGetFunction` | `test_ext_fb_null_get_fn` | NULL get callback → `INVALID_PARAM` |
| `ExternalFrameBufferTest::NullReleaseFunction` | `test_ext_fb_null_release_fn` | NULL release callback → `INVALID_PARAM` |
| `ExternalFrameBufferTest::SetAfterDecode` | `test_ext_fb_set_after_decode` | Setting FB functions after first decode → `ERROR` |
| `ExternalFrameBufferNonRefTest::ReleaseNonRefFrameBuffer` | `test_ext_fb_release_all_on_destroy` | All buffers released after decoder destroyed |
| `ExternalFrameBufferMD5Test::ExtFBMD5Match` | `test_ext_fb_md5_correctness` | MD5 matches with external frame buffers active |

### Category C: MD5 Correctness Against Test Vectors

**C source**: `test_vector_test.cc`, `test_vectors.cc`

| C Test | Rust Test | What It Verifies |
|---|---|---|
| `TestVectorTest::MD5Match` (single-thread) | `test_vector_md5_single_thread` | Per-frame MD5 matches reference for each test vector |
| `AV2MultiThreaded` (2–8 threads) | `test_vector_md5_multi_thread` | Same MD5 regardless of thread count |

The test vectors include:
- **64 quantizer variants** at 8-bit (`av1-1-b8-00-quantizer-{00..63}.ivf`)
- **64 quantizer variants** at 10-bit (`av1-1-b10-00-quantizer-{00..63}.ivf`)
- **Film grain** (`av1-1-b10-23-film_grain-50.ivf`, `av1-1-b8-23-film_grain-50.ivf`)
- **Various frame sizes** (16x16 through 226x226, including non-power-of-2 and odd sizes)
- **Feature-specific**: all-intra, CDF update, motion vectors, multi-frame MV, SVC layers
- **Container formats**: `.ivf` and `.mkv` (sizedown, sizeup)

### Category D: Multi-Threaded Decode Verification

**C source**: `decode_multithreaded_test.cc`

| C Test | Rust Test | What It Verifies |
|---|---|---|
| Encode then decode single-thread vs multi-thread | `test_mt_decode_matches_single_thread` | MD5 of multi-thread decode matches single-thread |
| Row-MT on/off | `test_mt_row_mt_toggle` | Row-level multithreading produces same output |
| Various tile configurations | `test_mt_tile_configurations` | Tiled decode works correctly with N threads |

### Category E: Error Path Testing

**C source**: `invalid_file_test.cc`

| C Test | Rust Test | What It Verifies |
|---|---|---|
| `InvalidFileTest` parametrized over invalid IVF files | `test_invalid_file_handling` | Decoder returns expected error codes for corrupted streams |
| Thread-safety of error handling | `test_invalid_file_multithreaded` | Error handling works with multiple threads |

Invalid test files from `test-data.sha1` include:
- `invalid-bug-1814.ivf` — specific regression
- `invalid-chromium-906381.ivf` — browser-discovered crash
- `invalid-oss-fuzz-*` — fuzzer-discovered crashes (dozens of these)
- `invalid-google-*` — internal regression tests
- Each has a matching `.ivf.res` file with expected error code per frame

---

## 3. Test Data Access from Rust

### Environment Variable

Rust tests should use the same `LIBAVM_TEST_DATA_PATH` environment variable:

```
LIBAVM_TEST_DATA_PATH=/path/to/testdata cargo test
```

### Path Resolution (test helper)

A `test_data_path()` helper should:
1. Check `LIBAVM_TEST_DATA_PATH` env var
2. Fall back to `../avm/out/` (relative to workspace root, where CMake downloads data)
3. Fall back to `.` (current directory)

### Test Vector Availability

Tests requiring test vectors should be gated with `#[ignore]` by default and run explicitly:

```
LIBAVM_TEST_DATA_PATH=/path/to/data cargo test -- --ignored
```

This prevents CI failures when test data hasn't been downloaded. A `download_test_data.sh` script should be provided.

### MD5 Reference File Format

Each `.ivf.md5` file contains one line per decoded frame:
```
<32-char-hex-md5>  img-<W>x<H>-<NNNN>.i420
```
Example:
```
a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6  img-352x288-0001.i420
```

The parser only needs the first field (the MD5 hex string).

### Invalid File Result Format

Each `.ivf.res` file contains one integer per line — the expected `avm_codec_err_t` value for each frame decode call.

---

## 4. Priority Order for Test Implementation

Ordered by safety criticality (highest first):

### P0 — Safety Invariants (implement first)
1. **Decode API null/invalid parameter rejection** — Ensures the Rust wrapper never passes invalid state to C
2. **Double-free prevention** — Verify `Drop` is called exactly once, no use-after-free
3. **Invalid file error handling** — Decoder doesn't crash/UB on malformed input (fuzzer regression tests)

### P1 — Correctness Foundation
4. **Single test vector MD5 verification** — Proves basic decode correctness with one known-good file
5. **IVF parser correctness** — The Rust `IvfReader` correctly parses headers and frames
6. **Frame data access safety** — Plane pointers, strides, dimensions are valid for all pixel formats

### P2 — Comprehensive Correctness
7. **Full test vector MD5 suite** — All 200+ vectors pass
8. **External frame buffer lifecycle** — Alloc/reuse/release/error paths
9. **Bit depth handling** — 8-bit and 10-bit, including high-bitdepth internal format with 8-bit output

### P3 — Threading & Robustness
10. **Multi-threaded decode correctness** — MD5 matches single-thread for 2, 4, 8 threads
11. **Multi-threaded error handling** — Invalid files with multiple threads
12. **Row-MT correctness** — Row-level parallelism produces identical output

### P4 — Extended Coverage
13. **External frame buffer MD5 verification** — Correct output with external FB management
14. **Performance regression tests** — Decode speed benchmarks (non-blocking, informational)
15. **Edge cases** — Very small frames (16x16), odd dimensions, monochrome

---

## 5. Rust Test Binary Structure

```
rustavm/
├── src/
│   └── lib.rs           # Unit tests for FFI bindings (existing)
├── tests/               # Integration tests
│   ├── common/
│   │   ├── mod.rs       # Re-exports
│   │   ├── test_data.rs # Path resolution, file loading, skip-if-missing
│   │   ├── md5.rs       # MD5 computation matching C's per-plane logic
│   │   └── ivf.rs       # IVF loading helpers for tests
│   ├── decode_api.rs    # Category A: parameter validation
│   ├── frame_buffer.rs  # Category B: external frame buffer lifecycle
│   ├── test_vectors.rs  # Category C: MD5 correctness
│   ├── multithreaded.rs # Category D: threading correctness
│   └── invalid_files.rs # Category E: error path / fuzz regression
├── benches/             # Criterion benchmarks (optional, P4)
│   └── decode_perf.rs
└── TEST_STRATEGY.md     # This document
```

### Test Module Details

#### `tests/common/test_data.rs`
- `fn test_data_path() -> PathBuf` — resolve LIBAVM_TEST_DATA_PATH
- `fn require_test_data(filename: &str) -> PathBuf` — returns path or panics with skip message
- `fn skip_if_no_test_data()` — for use with `#[ignore]` tests
- `const AV2_TEST_VECTORS: &[&str]` — mirror of C's `kAV2TestVectors` array

#### `tests/common/md5.rs`
- `fn frame_md5(frame: &Frame) -> String` — compute MD5 matching C's per-plane, per-row logic
- Must handle: `AVM_IMG_FMT_HIGHBITDEPTH` flag, chroma subsampling shifts, stride gaps
- Use the `md5` crate (pure Rust) — do NOT depend on C's `md5_utils.h`
- Critical: when `bit_depth == 8` but format has `HIGHBITDEPTH` flag, must downshift 16-bit samples to 8-bit before hashing (documented in `PORTING_PLAN.md`)

#### `tests/common/ivf.rs`
- Thin wrapper around `rustavm::ivf::IvfReader` for test convenience
- `fn decode_ivf(path: &Path, threads: Option<u32>) -> Vec<FrameData>` — full decode loop
- `fn decode_ivf_md5s(path: &Path, threads: Option<u32>) -> Vec<String>` — returns per-frame MD5 strings

#### `tests/decode_api.rs`
```
#[test] fn test_decoder_new()
#[test] fn test_decoder_with_threads()
#[test] fn test_decode_empty_slice()
#[test] fn test_decode_garbage_data()
#[test] fn test_decode_truncated_header()
#[test] fn test_decoder_drop_without_decode()
#[test] fn test_decoder_double_drop_safety()  // Ensure Drop is idempotent
#[test] fn test_get_frames_before_decode()     // Should return empty iterator
#[test] fn test_get_stream_info_before_decode()
```

#### `tests/frame_buffer.rs`
```
#[test] fn test_ext_fb_null_get_fn()
#[test] fn test_ext_fb_null_release_fn()
#[test] fn test_ext_fb_set_after_decode()
#[test] #[ignore] fn test_ext_fb_minimum_buffers()      // needs test data
#[test] #[ignore] fn test_ext_fb_insufficient_buffers()  // needs test data
#[test] #[ignore] fn test_ext_fb_no_release()            // needs test data
#[test] #[ignore] fn test_ext_fb_null_allocation()       // needs test data
#[test] #[ignore] fn test_ext_fb_undersized_buffer()     // needs test data
```

#### `tests/test_vectors.rs`
```
#[test] #[ignore] fn test_single_vector_md5()           // One vector as smoke test
#[test] #[ignore] fn test_all_8bit_quantizer_vectors()  // 64 quantizer-XX at 8-bit
#[test] #[ignore] fn test_all_10bit_quantizer_vectors() // 64 quantizer-XX at 10-bit
#[test] #[ignore] fn test_size_vectors()                // Various frame dimensions
#[test] #[ignore] fn test_feature_vectors()             // all-intra, cdf, mv, mfmv, svc
#[test] #[ignore] fn test_film_grain_vectors()          // Film grain synthesis
```

#### `tests/multithreaded.rs`
```
#[test] #[ignore] fn test_mt_2_threads_md5_match()
#[test] #[ignore] fn test_mt_4_threads_md5_match()
#[test] #[ignore] fn test_mt_8_threads_md5_match()
#[test] #[ignore] fn test_mt_various_vectors()
```

#### `tests/invalid_files.rs`
```
#[test] #[ignore] fn test_invalid_bug_1814()
#[test] #[ignore] fn test_invalid_chromium_906381()
#[test] #[ignore] fn test_invalid_oss_fuzz_files()     // Parametrized over all oss-fuzz files
#[test] #[ignore] fn test_invalid_with_threads()
```

---

## 6. Test Fixtures & Helpers to Create

### Required Helpers

| Helper | Location | Purpose |
|---|---|---|
| `test_data_path()` | `tests/common/test_data.rs` | Env var + fallback path resolution |
| `require_test_data(name)` | `tests/common/test_data.rs` | Load file or skip test |
| `frame_md5(frame)` | `tests/common/md5.rs` | Per-plane MD5 matching C logic |
| `read_md5_file(path)` | `tests/common/md5.rs` | Parse `.ivf.md5` → `Vec<String>` |
| `read_res_file(path)` | `tests/common/test_data.rs` | Parse `.ivf.res` → `Vec<i32>` |
| `decode_ivf_to_md5s(path, threads)` | `tests/common/ivf.rs` | Full decode loop → per-frame MD5 |
| `ExternalFrameBufferList` | `tests/frame_buffer.rs` | Rust port of C's `ExternalFrameBufferList` class |

### External Frame Buffer Test Fixture

The C `ExternalFrameBufferList` manages a pool of buffers with `in_use` tracking. The Rust equivalent needs:
- A `Vec<FrameBuffer>` where each entry has `data: Vec<u8>`, `in_use: bool`
- `extern "C"` get/release callbacks that cast `priv_` to `&mut ExternalFrameBufferList`
- Careful attention to aliasing: the `priv_` pointer will be accessed from C during decode

### Download Script

Create `rustavm/scripts/download_test_data.sh`:
- Parse `avm/test/test-data.sha1` for required files
- Download from `https://storage.googleapis.com/aom-test-data/<filename>`
- Verify SHA1 checksums
- Store to `LIBAVM_TEST_DATA_PATH` (default: `rustavm/testdata/`)

### Cargo Test Configuration

In `Cargo.toml`, add:
```toml
[dev-dependencies]
md5 = "0.7"    # Pure Rust MD5
```

### Test Running Commands

```bash
# Run unit tests only (no test data needed)
cargo test

# Run all tests including test-vector tests
LIBAVM_TEST_DATA_PATH=./testdata cargo test -- --ignored

# Run specific category
LIBAVM_TEST_DATA_PATH=./testdata cargo test test_vector -- --ignored

# Run with multiple threads
LIBAVM_TEST_DATA_PATH=./testdata cargo test multithreaded -- --ignored
```

---

## 7. Key Implementation Notes

### MD5 Plane-Walking Algorithm

The C MD5 computation (`md5_helper.h:24-46`) walks each plane row-by-row using stride, processing only the active pixel width (not the full stride). The Rust implementation must replicate this exactly:

```
for plane in 0..3:
    bytes_per_sample = 2 if HIGHBITDEPTH else 1
    h = if plane > 0: (d_h + y_chroma_shift) >> y_chroma_shift else: d_h
    w = if plane > 0: (d_w + x_chroma_shift) >> x_chroma_shift else: d_w
    w *= bytes_per_sample
    for row in 0..h:
        md5.update(plane_ptr[row * stride .. row * stride + w])
```

### High Bit-Depth Downshift

When `bit_depth == 8` but format includes `AVM_IMG_FMT_HIGHBITDEPTH`, the C tests call `avm_img_downshift` before MD5. The Rust wrapper should either:
1. Call `avm_img_downshift` via FFI (preferred — matches C exactly), or
2. Implement the truncation in Rust (simpler but must be verified)

### Test Data Is Large

The full test vector set is ~2.5GB. For CI, consider:
- A "smoke test" subset of 3-5 vectors covering 8-bit, 10-bit, and one odd size
- Full suite runs nightly or on-demand
- Mark full-suite tests with a cargo feature flag or separate test binary

### Thread Safety

`Decoder` is `!Send` and `!Sync` due to the internal C state. Multi-threaded tests should create separate `Decoder` instances per thread, not share one across threads. The C library handles its own internal threading.
