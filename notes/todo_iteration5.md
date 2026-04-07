# rustavm — TODO (Iteration 5 handover)

**Date:** 2026-04-07
**Source:** Re-audit after iteration 3/4 cleanup pass.
**Status:** Iteration 3 complete. R16, N5, N6 are done. N7 remains blocked.
This file lists what a fresh audit uncovered.

`cargo build`, `cargo test`, `cargo clippy --all-targets`, and
`cargo clippy --lib -- -W clippy::undocumented_unsafe_blocks` are all
clean. 61 tests pass / 22 ignored / 0 fail.

---

## Summary of iteration-3 results (do not redo)

### R16 — `uninlined_format_args` (33 instances)

Auto-fixed across `src/lib.rs` (1), `tests/md5_verification_test.rs` (29),
and `examples/simple_decode.rs` (3). Zero `uninlined_format_args` warnings
remain.

### N5 — `build.rs` define values

All five `.define()` calls (build.rs:25-29) now pass `"OFF"` instead of
`"0"`, matching CMake convention for boolean defines.

### N6 — `needless_range_loop` in `avmdec.rs`

The `for x in 0..w` loop was rewritten to
`for (x, byte) in row_buf.iter_mut().enumerate().take(w)`, eliminating
the clippy warning while preserving truncation behavior.

### QA verification

| Check | Result |
|-------|--------|
| `cargo check` | PASS |
| `cargo test` | 61 passed, 0 failed, 22 ignored |
| `cargo clippy --lib -- -W undocumented_unsafe_blocks` | 0 warnings |
| `cargo clippy --all-targets` | **0 warnings** |
| `rust-version` in Cargo.toml | `"1.74"` present |

---

## Remaining work

### S1 — `decode_to_md5s` missing flush call 🟡 MEDIUM (correctness)

**File:** `tests/md5_verification_test.rs:156-172`
**Why:** The test helper decodes all IVF packets but never calls
`decoder.flush()` afterward.  If a test vector uses B-frame reordering,
the final reordered frames are never retrieved, producing an MD5 list
that is silently too short.  The binary `src/bin/avmdec.rs:191-198`
correctly flushes and drains post-flush frames.

**How to apply:**
```rust
pub fn decode_to_md5s(
    ivf_path: &Path,
    threads: Option<u32>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let file = File::open(ivf_path)?;
    let mut ivf = IvfReader::new(BufReader::new(file))?;
    let mut decoder = Decoder::with_config(threads)?;
    let mut md5s = Vec::new();

    while let Some(pkt) = ivf.next_frame()? {
        decoder.decode(&pkt.data)?;
        for frame in decoder.get_frames() {
            md5s.push(frame_md5(&frame));
        }
    }

    // Flush to retrieve any buffered frames (B-frame reordering).
    decoder.flush()?;
    for frame in decoder.get_frames() {
        md5s.push(frame_md5(&frame));
    }

    Ok(md5s)
}
```

**Priority:** Medium. Silent data loss in test infrastructure. The
current AV2 test vectors are all-intra or low-delay, so this hasn't
manifested yet, but will bite when B-frame vectors are added.

---

### S2 — Library pulls in binary-only dependencies 🟡 MEDIUM (packaging)

**File:** `Cargo.toml`
**Why:** `md5 = "0.7.0"` and `clap = { version = "4", features = ["derive"] }`
are in `[dependencies]`.  They are only used by `src/bin/avmdec.rs`
(both) and `tests/md5_verification_test.rs` (`md5` only).  Any
downstream crate that depends on `rustavm` transitively pulls in
`clap`'s full derive macro stack (~15 crates), even though it only
needs the FFI decoder.

**How to apply (option A — feature-gated binary):**
```toml
[features]
default = []
bin = ["dep:md5", "dep:clap"]

[dependencies]
md5 = { version = "0.7.0", optional = true }
clap = { version = "4", features = ["derive"], optional = true }

[dev-dependencies]
md5 = "0.7.0"
```

Then gate `src/bin/avmdec.rs` on the `bin` feature:
```toml
[[bin]]
name = "avmdec"
required-features = ["bin"]
```

Users install with `cargo install rustavm --features bin`.  Library
consumers get zero transitive binary deps.

**How to apply (option B — workspace split):**
Move `avmdec` into a separate `avmdec/` workspace member with its own
`Cargo.toml` that depends on `rustavm`.  Cleaner but requires a workspace.

**Priority:** Medium. Not a correctness issue, but a packaging hygiene
issue that will matter at `crates.io` publish time.  Option A is ~5
minutes of work.

---

### N7 — Add `repository` field to Cargo.toml 🟢 LOW (carried)

**File:** `Cargo.toml`
**Why:** Standard metadata for crates.io publishing. Blocked on author
confirming the canonical URL.

---

### N8 — `extern "C-unwind"` on callback signatures 🟢 LOW

**Files:** `src/decoder.rs:194-202`, `tests/frame_buffer_test.rs:222-253`
**Why:** Current MSRV (1.74) exceeds the stabilization point for
`extern "C-unwind"` (1.71).  Changing the callback signatures in
`set_frame_buffer_functions` and the test callbacks from `extern "C"`
to `extern "C-unwind"` gives defined behavior if a panic unwinds
through C frames.  Currently the codebase documents this UB risk in
comments at `decoder.rs:250-255` and `decoder.rs:185-191` but does
not mitigate it.

**How to apply:**
```rust
// decoder.rs — set_frame_buffer_functions signature
pub unsafe fn set_frame_buffer_functions(
    &mut self,
    get_fb: unsafe extern "C-unwind" fn(
        priv_: *mut c_void,
        min_size: usize,
        fb: *mut avm_codec_frame_buffer_t,
    ) -> c_int,
    release_fb: unsafe extern "C-unwind" fn(
        priv_: *mut c_void,
        fb: *mut avm_codec_frame_buffer_t,
    ) -> c_int,
    priv_: *mut c_void,
) -> Result<(), DecoderError> {
```

And update the corresponding `extern "C"` callbacks in
`tests/frame_buffer_test.rs` to `extern "C-unwind"`.

**Note:** This is a breaking API change for any existing users passing
`extern "C"` callbacks.  Acceptable at 0.1.x.

**Priority:** Low.  Theoretical until someone writes a panicking
callback, but the fix is mechanical and aligns the code with its own
safety documentation.

---

### N9 — `md5_verification_test.rs` hardcodes `HIGHBITDEPTH_FLAG` 🟢 VERY LOW

**File:** `tests/md5_verification_test.rs:32`
**Why:** The test file defines `const HIGHBITDEPTH_FLAG: u32 = 0x800`
locally instead of using `rustavm::AVM_IMG_FMT_HIGHBITDEPTH`.  The
comment says "reproduced here to keep the test self-contained", but
if the C header value ever changes, the test will silently use the
wrong constant.  `rustavm::AVM_IMG_FMT_HIGHBITDEPTH` is a public
re-export and always matches the bindgen output.

**How to apply:** Replace the local constant with the re-export:
```rust
// Delete: const HIGHBITDEPTH_FLAG: u32 = 0x800;
// Use: rustavm::AVM_IMG_FMT_HIGHBITDEPTH
```

And update all references (line 104, and the comment block).

**Priority:** Very low.  The AV2 spec is unlikely to change this
value, but the fix is one line.

---

## Suggested PR breakdown

**Single PR — `cleanup/iteration-5`:**

1. S1: Add flush + drain to `decode_to_md5s` (correctness, ~2 min)
2. S2: Feature-gate binary-only deps (packaging, ~5 min)
3. N8: `extern "C-unwind"` on callback signatures (safety, ~5 min)
4. N9: Use re-exported `HIGHBITDEPTH_FLAG` constant (~1 min)
5. N7: Cargo.toml `repository` (if URL confirmed)

After this PR, the crate would have:
- Correct flush behavior in test infrastructure
- Zero unnecessary transitive deps for library consumers
- Defined behavior for panic-through-C on callback paths
- Zero clippy warnings on `--all-targets`
- 61+ tests passing, full validation coverage
- A fuzz target (manual run, not CI)

---

## Out of scope (carried from iteration 2)

These items were identified during review but deferred as non-trivial
or requiring broader discussion:

1. **Miri / ASAN CI** — Add CI jobs running Miri on safe-code tests
   and ASAN on FFI integration tests. Requires CI infrastructure.
2. **FourCC allow-list** — The IVF parser currently accepts any
   FourCC. Consider restricting to known AV2 codec tags.
3. **`avm_codec_error_to_string` / `avm_codec_error_detail`
   integration** — Would improve `DecoderError` messages. Nice-to-have.
