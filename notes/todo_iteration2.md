# rustavm — TODO (Iteration 2 handover)

**Date:** 2026-04-07
**Source:** Re-audit after iteration 1 hardening pass.
**Status:** Iteration 1 substantially complete. 18 of the 24 R-items from
`todo_iteration1.md` are done. This file lists only what is left.

`cargo build`, `cargo test`, and `cargo clippy --lib -- -W
clippy::undocumented_unsafe_blocks` are all clean. 55 tests pass / 24
ignored / 0 fail. The remaining work is short enough to land in a single
PR.

---

## Summary of iteration-1 results (do not redo)

| Done | Notes |
|---|---|
| R1, R2 (IVF cap + header validation) | `ivf.rs` rewritten — 10 new tests in `tests/ivf_validation_test.rs` |
| R3 (`Decoder: !Send + !Sync` explicit) | `PhantomData<*const ()>` in `decoder.rs:38-41` |
| R4 (HBD stride byte check) | `decoder.rs:359-363` using `AVM_IMG_FMT_HIGHBITDEPTH` |
| R5 (`MaybeUninit` for FFI structs) | `decoder.rs:50-77, 152-161` |
| R6 (always pass `&cfg`) | `decoder.rs:67` |
| R7 (`avmdec.rs` checked plane indexing) | `chunks(stride).take(h)` + `.get()` in both pre- and post-flush loops |
| R8 (`// SAFETY:` comments) | All 17 unsafe blocks annotated; clippy clean |
| R10 (`Frame::img: NonNull<avm_image_t>`) | `decoder.rs:273`, debug_asserts removed |
| R11 (`FrameBufferManager` deleted) | Trait and field gone; tests use raw FFI directly |
| R12 (`AVM_IMG_FMT_HIGHBITDEPTH` constant) | All three call sites |
| R13 (`Decoder::flush()` + CLI drain) | `decoder.rs:118-134`, `avmdec.rs:182-184` |
| R14 (`BufWriter`) | `avmdec.rs:40-44`, 1 MiB capacity |
| R15 (drop hardcoded GCC path) | `build.rs:6-17, 42-44` runtime detection via `gcc -print-file-name=include` |
| R17 (rerun-if-changed) | `build.rs:20-22` — partial but functional |
| R18 (clap CLI) | `avmdec.rs:1-24` derive parser |
| R19 (`IvfReader::header()` accessor) | `ivf.rs:78-80` |
| R20 (bindgen `allow`s scoped to `mod sys`) | `lib.rs:1-10` |
| R21 (Cargo.toml metadata) | Added `description`, `license`, `keywords`, `categories` |
| R22 (`.gitignore`) | `target/`, `*.y4m`, `*.yuv`, `Cargo.lock` |
| R23 (fuzz target) | `fuzz/fuzz_targets/ivf_decode.rs` covers parser, decoder, plane accessors, flush |

---

## Remaining work

### R9-bis — Replace plane validation test stubs with real tests 🟡 MEDIUM (test gap)

**File:** `tests/plane_validation_test.rs`
**Why:** Iteration 1 added six new validation branches in `Frame::plane()`
(`index >= 3`, null pointer, negative stride, `stride < row_bytes`,
`checked_mul` overflow, `len > sz`). The current test file contains two
`#[ignore]`'d tests with **empty bodies** and a TODO comment. Coverage of
the new validation logic is currently provided only by the fuzz target,
which is not in CI.

The author marked the tests as needing test data, but this is wrong:
`avm_image_t` is a `repr(C)` plain-data struct exposed by the bindings
and can be constructed directly under `#[cfg(test)]` without decoding
anything.

**Blocker:** `Frame` cannot currently be constructed from outside
`decoder.rs` — `FrameIterator::next` is the only constructor.

**How to apply:**

1. Add a `#[cfg(test)]` constructor to `Frame` in `src/decoder.rs`:
   ```rust
   impl<'a> Frame<'a> {
       #[cfg(test)]
       pub(crate) fn from_raw_for_test(img: NonNull<avm_image_t>) -> Frame<'a> {
           Frame { img, _marker: PhantomData }
       }
   }
   ```

2. Move `plane_validation_test.rs` into `src/decoder.rs` as a `#[cfg(test)]
   mod tests` block (so it can use `pub(crate)` items), or expose the
   constructor as `pub` behind a `#[doc(hidden)]` attribute.

3. Write the six tests, each constructing an `avm_image_t` via
   `MaybeUninit::zeroed`, setting only the fields under test, and verifying
   that `Frame::plane(index)` returns `None`:

   - `test_plane_rejects_index_out_of_bounds` — `frame.plane(3)` → `None`
   - `test_plane_rejects_null_plane_pointer` — `img.planes[0] = ptr::null_mut()` → `None`
   - `test_plane_rejects_negative_stride` — `img.stride[0] = -1` → `None`
   - `test_plane_rejects_stride_below_row_bytes` — set `d_w=320, fmt=I42016, stride[0]=320` (instead of 640) → `None`
   - `test_plane_rejects_stride_height_overflow` — `stride[0] = i32::MAX, d_h = i32::MAX as u32` → `None` (via `checked_mul`)
   - `test_plane_rejects_len_exceeds_sz` — `stride[0]*d_h = 1MB`, `img.sz = 1024` → `None`

4. Add one positive test that constructs a valid mock image and verifies
   `plane(0)` returns `Some(slice)` of the right length, to confirm the
   constructor works.

**Note:** Each test must keep its backing `Vec<u8>` alive for the duration
of the call so the plane pointer doesn't dangle. Use a local `let mut buf
= vec![0u8; ...];` in each test and set `img.planes[0] = buf.as_mut_ptr()`.

**Priority:** Medium. Brand-new validation logic with zero non-fuzz
coverage. Estimated effort: 30 minutes.

---

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

**Priority:** Low. Mechanical change, ~30 seconds.

---

### N1 — `lib.rs::test_decoder_init` still uses `mem::zeroed` 🟢 LOW

**File:** `src/lib.rs:30`
**Why:** Iteration 1 replaced production `mem::zeroed` with `MaybeUninit`
in `decoder.rs`, but the unit test in `lib.rs` still does:
```rust
let mut ctx: avm_codec_ctx_t = std::mem::zeroed();
```
Inconsistent with the production pattern, and a future grep for
`mem::zeroed` would still flag this file.

**How to apply:** Mirror the production pattern:
```rust
let mut ctx = std::mem::MaybeUninit::<avm_codec_ctx_t>::uninit();
let res = avm_codec_dec_init_ver(
    ctx.as_mut_ptr(), iface, std::ptr::null(), 0,
    AVM_DECODER_ABI_VERSION as i32);
assert_eq!(res, avm_codec_err_t_AVM_CODEC_OK);
let mut ctx = ctx.assume_init();
avm_codec_destroy(&mut ctx);
```

**Priority:** Low. ~2 minutes.

---

### N3 — Refactor duplicated plane-write logic in `avmdec.rs` 🟢 LOW

**File:** `src/bin/avmdec.rs:116-172` (per-input-frame loop) and
`avmdec.rs:185-243` (post-flush drain loop)
**Why:** ~30 lines of plane-iteration code are duplicated between the
two loops. Iteration 1 added the post-flush drain (R13) by copy-paste,
which works but doubles the surface area for future bug fixes.

**How to apply:** Factor into a helper:
```rust
fn write_decoded_frame<W: Write>(
    img: &Frame,
    out: &mut Option<BufWriter<W>>,
    md5: &mut Option<&mut md5::Context>,
    raw_video: bool,
) -> io::Result<()> { ... }
```

The first-frame Y4M-header logic still belongs to the input loop (only
runs once), but the per-frame `FRAME\n` + plane-write block is identical
in both loops.

**Priority:** Low (refactor, not bug). ~15 minutes. Optional.

---

### N4 — `Decoder::decode` doc comment is slightly inaccurate 🟢 VERY LOW

**File:** `src/decoder.rs:84`
**Why:** Says *"Decode one frame of compressed data"*. The C API actually
accepts an arbitrary OBU sequence, and a single call may produce zero,
one, or many output frames.

**How to apply:** Change to:
> *"Submit compressed data to the decoder. The data may contain zero or
> more frames; call [`get_frames`] after this returns to retrieve any
> decoded output."*

**Priority:** Very low. ~1 minute. Doc nit.

---

### Cargo.toml — `repository` and `rust-version` 🟢 LOW

**File:** `Cargo.toml`
**Why:** Iteration 1 added `description`, `license`, `keywords`, and
`categories` but skipped two more standard fields.

**How to apply:**
```toml
repository = "https://example.com/path/to/rustavm"  # confirm with author
rust-version = "1.74"                                # confirm by `cargo msrv` or trial
```

**Priority:** Low. ~1 minute.

---

## Suggested PR breakdown

**Single PR — `cleanup/iteration-2`:**

Bundle all six items above into one PR. Total estimated effort: under one
hour, dominated by R9-bis. Order of commits within the PR:

1. R16: CMake `"0"` → `"OFF"` (one-line, low-risk)
2. N1: lib.rs test uses `MaybeUninit` (mirrors production)
3. R9-bis: add `#[cfg(test)]` `Frame` constructor + six validation tests
4. N3 (optional): factor `write_decoded_frame` helper in `avmdec.rs`
5. N4: fix `decode()` doc comment
6. Cargo.toml `repository` + `rust-version`

After this PR, I would tag `0.1.0`. The crate then has:
- No critical or high-severity unsoundness
- No untouched DoS vectors
- Test coverage on every validation branch added in iteration 1
- A fuzz target (manual run, not CI)
- Build clean on stable, clippy clean on `-W
  clippy::undocumented_unsafe_blocks`

---

## Out of scope (deferred)

- **`extern "C-unwind"` / `catch_unwind` callback wrapper.** Theoretical
  until someone writes a panicking callback. Documented in `decoder.rs`.
- **Miri / ASAN in CI.** Requires updating the local nightly toolchain.
  Run periodically by hand instead.
- **Optional FourCC allow-list in `IvfReader`.** Iteration 1 left this
  out deliberately (would need a `strict: bool` flag). Add only if
  there is a concrete use case.
- **`avm_codec_error_to_string` / `avm_codec_error_detail` integration.**
  Would improve `DecoderError` messages. Nice-to-have, not required.
