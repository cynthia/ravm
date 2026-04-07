# rustavm — TODO (Iteration 3 handover)

**Date:** 2026-04-07
**Source:** Re-audit after iteration 2 hardening pass.
**Status:** Iteration 2 complete. 5 of the 6 items from
`todo_iteration2.md` are done. R16 (CMake `"0"` -> `"OFF"`) was not
addressed. This file lists what remains.

`cargo build`, `cargo test`, and `cargo clippy --lib -- -W
clippy::undocumented_unsafe_blocks` are all clean. 61 tests pass / 22
ignored / 0 fail.

---

## Summary of iteration-2 results (do not redo)

### Backend agent — `src/decoder.rs`

| Done | Notes |
|---|---|
| R9-bis (plane validation tests) | Added `Frame::from_raw_for_test` constructor (line 456) and `mod plane_validation_tests` (line 465) with 7 tests covering all 6 validation branches plus one positive test. Deleted `tests/plane_validation_test.rs`. |

**Details:**

- `from_raw_for_test` is `#[cfg(test)] pub(crate)`, constructed from a
  `NonNull<avm_image_t>` — no decoder needed for unit testing.
- `zeroed_image()` helper uses `MaybeUninit::zeroed()` to produce a
  repr(C) all-zero image struct.
- `test_plane_rejects_stride_height_overflow` is gated with
  `#[cfg(target_pointer_width = "32")]` because on 64-bit the overflow is
  structurally unreachable: `i32::MAX * u32::MAX < usize::MAX`.
- Each test that sets a plane pointer keeps its backing `Vec<u8>` alive
  for the duration of the call, preventing dangling pointers.
- All 7 tests (6 negative, 1 positive) pass on `cargo test`.

### Frontend agent — `src/bin/avmdec.rs`

| Done | Notes |
|---|---|
| N3 (refactor duplicated plane-write logic) | Extracted `write_frame` helper (line 26). Called at line 182 (decode loop) and line 196 (post-flush drain loop). |

**Details:**

- `write_frame` takes `&Frame`, `&mut Option<BufWriter<File>>`,
  `&mut md5::Context`, `compute_md5: bool`, `raw_video: bool`.
- Consolidates the `FRAME\n` header write, per-plane iteration with
  stride/width/height, HBD downshift logic, and md5 consumption into one
  function.
- The first-frame Y4M header logic remains in the main decode loop (runs
  once only), as designed.
- No duplicated plane-write blocks remain in the file.
- Compiles cleanly. The pre-existing `needless_range_loop` clippy warning
  at line 53 (inside `write_frame`) is inherited from the original code.

### Lead agent — `src/lib.rs`, `src/decoder.rs`, `Cargo.toml`

| Done | Notes |
|---|---|
| N1 (lib.rs `MaybeUninit`) | `test_decoder_init` (line 28) now uses `MaybeUninit::<avm_codec_ctx_t>::uninit()` + `assume_init()` instead of `mem::zeroed()`. |
| N4 (decode doc comment) | `Decoder::decode` (line 84) now reads: *"Submit compressed data to the decoder. The data may contain zero or more frames; call `get_frames` after this returns to retrieve any decoded output."* |
| Cargo.toml `rust-version` | `rust-version = "1.74"` at line 9. `repository` field omitted (no canonical URL confirmed). |

**Details:**

- N1: The production pattern in `decoder.rs:50-77` already used
  `MaybeUninit`. The test now mirrors it exactly. Zero `mem::zeroed` calls
  remain in the crate.
- N4: The old comment (*"Decode one frame of compressed data"*) was
  misleading because the C API accepts arbitrary OBU sequences. The new
  comment accurately describes multi-frame data submission.
- Cargo.toml: `description`, `license`, `keywords`, `categories` were
  already present from iteration 1. `rust-version = "1.74"` confirms the
  MSRV. The `repository` field was deferred per instructions (no
  confirmed URL).

### QA agent — verification

Ran 10 verification steps post-implementation:

| Step | Check | Result |
|---|---|---|
| 1 | `cargo check` | PASS |
| 2 | `cargo test` | 61 passed, 0 failed, 22 ignored |
| 3 | `cargo clippy --lib -- -W clippy::undocumented_unsafe_blocks` | 0 warnings |
| 4 | `cargo clippy --all-targets` | Pre-existing warnings only |
| 5 | `tests/plane_validation_test.rs` deleted | PASS |
| 6 | `from_raw_for_test` + `mod plane_validation_tests` in decoder.rs | PASS |
| 7 | `MaybeUninit` in lib.rs, no `mem::zeroed` | PASS |
| 8 | `write_frame` helper, no duplication in avmdec.rs | PASS |
| 9 | `rust-version` in Cargo.toml | PASS |
| 10 | `build.rs` CMake defines (R16 — not addressed) | `"0"` unchanged |

Pre-existing clippy warnings (not introduced by iteration 2):
- `uninlined_format_args` — 29 in `tests/md5_verification_test.rs`, 3 in
  `examples/simple_decode.rs`, 1 in `src/lib.rs` test code
- `needless_range_loop` — 1 in `src/bin/avmdec.rs:53` (HBD downshift loop)

---

## Remaining work

### R16 — CMake bool defines should be `"OFF"` not `"0"` 🟢 LOW

**File:** `build.rs:25-29`
**Why:** Currently passes string `"0"` for five flags. CMake's `if(...)`
treats `"0"` as falsy *by allow-list*, but `if(DEFINED CONFIG_AV2_ENCODER)`
sees the variable as defined, and any AV2 cmake check using the latter
form will silently build the encoder anyway.

**How to apply:**
```rust
.define("CONFIG_AV2_ENCODER", "OFF")
.define("ENABLE_EXAMPLES",    "OFF")
.define("ENABLE_TESTS",       "OFF")
.define("ENABLE_TOOLS",       "OFF")
.define("ENABLE_DOCS",        "OFF")
```

**Priority:** Low. Mechanical change, ~30 seconds. Carried over from
iteration 2 (not addressed).

---

### N5 — Fix `needless_range_loop` clippy warning in `avmdec.rs` 🟢 LOW

**File:** `src/bin/avmdec.rs:53`
**Why:** The HBD downshift loop `for x in 0..w { row_buf[x] = ... }` triggers
`clippy::needless_range_loop`. Can be replaced with an iterator pattern.

**How to apply:**
```rust
for (x, byte) in row_buf.iter_mut().enumerate().take(w) {
    *byte = match row_data.get(x * 2) {
        Some(&b) => b,
        None => { eprintln!("Warning: truncated plane data at row"); 0 }
    };
}
```
Or more idiomatically, use `zip` on `row_buf` and a stepped iterator over
`row_data`.

**Priority:** Low. Pre-existing, not introduced by iteration 2. ~2 minutes.

---

### N6 — Inline format args in test files 🟢 VERY LOW

**Files:** `tests/md5_verification_test.rs` (29 warnings),
`examples/simple_decode.rs` (3 warnings), `src/lib.rs` test (1 warning)
**Why:** `clippy::uninlined_format_args` fires 33 times across test/example
code. These are cosmetic but make `cargo clippy --all-targets` noisy.

**How to apply:** `cargo clippy --fix --tests --examples` or manual
`format!("{foo}")` → inline variable capture.

**Priority:** Very low. Cosmetic. ~5 minutes for the whole batch.

---

### N7 — Add `repository` field to Cargo.toml 🟢 LOW

**File:** `Cargo.toml`
**Why:** Standard metadata field for crates.io publishing. Skipped in
iteration 2 because no canonical URL was confirmed.

**How to apply:**
```toml
repository = "https://..." # confirm with author
```

**Priority:** Low. Blocked on author confirming the URL.

---

## Suggested PR breakdown

**Single PR — `cleanup/iteration-3`:**

Bundle all items above into one PR. Total estimated effort: under 15
minutes, dominated by N6 if done manually.

1. R16: CMake `"0"` -> `"OFF"` (one-line, low-risk)
2. N5: fix `needless_range_loop` in avmdec.rs
3. N6: inline format args across test/example files
4. N7: Cargo.toml `repository` (if URL confirmed)

After this PR, the crate would have:
- Zero clippy warnings on `--all-targets`
- No critical or high-severity unsoundness
- No untouched DoS vectors
- Test coverage on every validation branch added in iteration 1
- Build clean on stable, clippy clean on all targets
- A fuzz target (manual run, not CI)

---

## Out of scope (deferred)

Carried over from iteration 2:

- **`extern "C-unwind"` / `catch_unwind` callback wrapper.** Theoretical
  until someone writes a panicking callback. Documented in `decoder.rs`.
- **Miri / ASAN in CI.** Requires updating the local nightly toolchain.
  Run periodically by hand instead.
- **Optional FourCC allow-list in `IvfReader`.** Iteration 1 left this
  out deliberately (would need a `strict: bool` flag). Add only if
  there is a concrete use case.
- **`avm_codec_error_to_string` / `avm_codec_error_detail` integration.**
  Would improve `DecoderError` messages. Nice-to-have, not required.
