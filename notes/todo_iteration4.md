# Iteration 4 TODO — rustavm

Date: 2026-04-07

## Iteration 3 Summary

Iteration 3 resolved all remaining clippy warnings, bringing the crate to **zero warnings on `--all-targets`**.

### Completed

- **R16 — `uninlined_format_args`**: Auto-fixed all 33 instances across `src/lib.rs` (1), `tests/md5_verification_test.rs` (29), and `examples/simple_decode.rs` (3).
- **N5 — `build.rs` define values**: All five `.define()` calls (build.rs:25-29) now pass `"OFF"` instead of `"0"`, matching CMake convention for boolean defines.
- **N6 — `needless_range_loop`**: The `for x in 0..w` loop in `src/bin/avmdec.rs` was rewritten to `for (x, byte) in row_buf.iter_mut().enumerate().take(w)`, eliminating the warning while preserving truncation behavior.

### Skipped

- **N7 — `repository` field in Cargo.toml**: Blocked on author confirming the canonical repository URL.

## Current Crate Status

- `cargo check`: clean
- `cargo test`: 61 passed, 0 failed, 22 ignored (ignored tests require `LIBAVM_TEST_DATA_PATH` or avoid SIGABRT-prone C codec paths)
- `cargo clippy --lib -- -W clippy::undocumented_unsafe_blocks`: 0 warnings
- `cargo clippy --all-targets`: **0 warnings**
- `rust-version = "1.74"` set in Cargo.toml (from iteration 2)

## Remaining Work

### Blocked

- **N7 — `repository` URL in Cargo.toml**: Waiting on author to confirm canonical URL.

### Out-of-Scope (carried from iteration 2)

These items were identified during review but deferred as non-trivial or requiring broader discussion:

1. **`extern "C-unwind"`** — Replace `extern "C"` with `extern "C-unwind"` on FFI callbacks to get defined behavior if the C codec `longjmp`s or aborts through Rust frames. Requires MSRV bump to 1.71+ (already satisfied by current 1.74).
2. **Miri / ASAN CI** — Add CI jobs running Miri on safe-code tests and ASAN on FFI integration tests to catch undefined behavior.
3. **FourCC allow-list** — The IVF parser currently accepts any FourCC. Consider restricting to known AV2 codec tags.
4. **Error string integration** — `avm_codec_err_to_string` is available in the C API but not yet exposed through the Rust error types.
