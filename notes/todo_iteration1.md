# rustavm — TODO (Iteration 1 handover)

**Date:** 2026-04-07
**Source:** Re-audit after Phase 1 hardening pass.
**Status:** Phase 1 (UB / leak elimination) substantially complete. Phase 2
(DoS hardening, dead-code cleanup, build hygiene) is incomplete and is the
focus of this list.

`cargo build` and `cargo test` are clean (39 pass / 20 ignored / 0 fail).
`cargo clippy --lib -- -W clippy::undocumented_unsafe_blocks` reports 17
warnings, all in `src/decoder.rs`.

---

## Done in prior pass — do not redo

| ID | Fix | Location |
|---|---|---|
| C1 | `mem::transmute` of FFI callbacks removed; `set_frame_buffer_functions` is `unsafe fn` with typed `c_int` callbacks | `src/decoder.rs:155-182` |
| C2 | `slice::from_raw_parts` validation: index, null, negative-stride, `stride < plane_w`, `checked_mul`, `len > sz` | `src/decoder.rs:297-336` |
| C3 | Negative stride explicit reject | `src/decoder.rs:309-312, 351-356` |
| C4 | `Frame<'a>` lifetime is correct as written; borrow checker enforces "no `decode()` while frames alive". libavm convention guarantees frame stability between `decode()` calls. (False alarm in prior critique.) | `src/decoder.rs:108-115, 185-206` |
| S1 | `String::leak()` removed; replaced with `Cow<'static, str>` | `src/bin/avmdec.rs:56-86` |
| M3 | Chroma shift clamped via `.min(31)` to prevent UB | `src/decoder.rs:373, 394` |
| Tests | 39 always-run + 20 data-gated; compile-time regression test for transmute removal (`test_callback_type_safety_no_transmute`) | `tests/frame_buffer_test.rs:332-347` |

---

## MUST-FIX before declaring safe for untrusted input

These are tracked by severity, not order. Item **R1** is the single
highest-leverage outstanding fix.

### R1 — IVF reader: cap frame size and use fallible allocation 🔴 HIGH (DoS)

**File:** `src/ivf.rs:53-60`
**Why:** A hostile or corrupted IVF can claim a `u32::MAX` frame size and
trigger a single `vec![0u8; ~4 GiB]` allocation. This is the most exploitable
remaining bug and the easiest to fix. Author's own `FINAL_AUDIT.md` calls
this out as **H-2 UNRESOLVED** but didn't touch the file.

**How to apply:**
```rust
const MAX_IVF_FRAME_BYTES: usize = 64 * 1024 * 1024;

let size = u32::from_le_bytes(size_buf) as usize;
if size > MAX_IVF_FRAME_BYTES {
    return Err(io::Error::new(io::ErrorKind::InvalidData,
        "IVF frame size exceeds limit"));
}
let mut data = Vec::new();
data.try_reserve_exact(size)
    .map_err(|e| io::Error::new(io::ErrorKind::OutOfMemory, e))?;
data.resize(size, 0);
self.reader.read_exact(&mut data)?;
```

Make the cap configurable: `IvfReader::with_max_frame_size(reader, max)`.

---

### R2 — IVF header validation 🔴 HIGH

**File:** `src/ivf.rs:23-43`
**Why:** Currently the parser reads `version` (offset 4-5) and
`header_length` (offset 6-7) into the buffer and discards them. FourCC is
captured but never compared against an allow-list. A non-AV2 IVF, an IVF with
extended header, or an IVF with `version != 0` is silently misparsed.

**How to apply:**
```rust
let version = u16::from_le_bytes([buf[0], buf[1]]);
if version != 0 {
    return Err(io::Error::new(io::ErrorKind::InvalidData,
        format!("unsupported IVF version {version}")));
}
let header_len = u16::from_le_bytes([buf[2], buf[3]]);
if header_len != 32 {
    return Err(io::Error::new(io::ErrorKind::InvalidData,
        format!("unexpected IVF header length {header_len}")));
}
if header.width == 0 || header.height == 0 {
    return Err(io::Error::new(io::ErrorKind::InvalidData,
        "IVF header has zero dimension"));
}
```

Optional behind a `strict: bool` constructor flag: validate FourCC against
the AV2 allow-list (verify the exact bytes from the C reference encoder
output before pinning the list).

---

### R3 — Make `Decoder` `!Send + !Sync` explicit 🟠 MEDIUM (regression hazard)

**File:** `src/decoder.rs:35-41`
**Why:** Empirically `Decoder` is currently `!Send + !Sync`, **but only by
accident**: the `ext_fb_manager: Option<Box<dyn FrameBufferManager>>` field
defaults `dyn FrameBufferManager` to `+ 'static` without Send/Sync, which
defeats the auto-trait. The moment that field is removed (it is currently
`#[allow(dead_code)]`) or rewritten to `dyn FrameBufferManager + Send + Sync`,
auto-traits flip and `Decoder` becomes movable across threads, racing the
libavm internal context. Author's audit (M-3) flags the *documentation* gap
but misses that the *enforcement* is incidental.

**How to apply:**
```rust
pub struct Decoder {
    ctx: avm_codec_ctx_t,
    #[allow(dead_code)]
    ext_fb_manager: Option<Box<dyn FrameBufferManager>>,
    /// Explicit !Send + !Sync marker. The libavm decoder context contains
    /// internal mutable state that is not safe to access from multiple
    /// threads concurrently. Multi-threaded decoding inside libavm is
    /// configured via `avm_codec_dec_cfg_t::threads`, not via Rust threading.
    _not_send: PhantomData<*const ()>,
}
```

Add a `///` doc comment to `struct Decoder` explaining the threading model.

---

### R4 — Stride byte check (currently uses pixel count) 🟠 MEDIUM (panic / DoS)

**File:** `src/decoder.rs:316-321`
**Why:** `plane()` checks `stride < plane_w` where `plane_w` is in
**pixels**. For 10/12-bit formats, valid stride is `>= 2 * plane_w`. A
corrupted high-bit-depth metadata triple `(stride=plane_w, height=h,
sz=stride*h)` passes all current `plane()` validations, so `plane()` returns
`Some` with a too-small slice. The CLI row loop (`avmdec.rs:131`,
`plane[start + x*2]`) then panics on bounds check. Author's audit flags this
as **N-1**.

**How to apply:**
```rust
// stride is in bytes; plane_w is in pixels.
let bps = if (*self.img).fmt & rustavm::AVM_IMG_FMT_HIGHBITDEPTH != 0 { 2 } else { 1 };
let plane_w = self.plane_width(index);
let row_bytes = plane_w.checked_mul(bps)?;
if stride < row_bytes {
    return None;
}
```

Use the bindgen-generated `AVM_IMG_FMT_HIGHBITDEPTH` constant. Same constant
is needed for **R7** (avmdec.rs hardening).

---

### R5 — `mem::zeroed()` on FFI structs 🟠 MEDIUM

**File:** `src/decoder.rs:56-57, 125`
**Why:** Three `std::mem::zeroed::<T>()` calls. Today they happen to be sound
because `avm_codec_ctx_t` / `avm_codec_dec_cfg_t` / `avm_codec_stream_info_t`
contain only integers and `Option<unsafe extern "C" fn ...>` (whose niche
makes zero a valid `None`). The first time bindgen output gains a `NonNull`,
a `&'static`, an enum without a 0 discriminant, or a `bool`, every
constructor in this file becomes UB and the test suite will not catch it.
Author lists this as **L-1 Low**, but the consequence on a struct change is
silent UB, not just "fragility."

**How to apply:**
```rust
use std::mem::MaybeUninit;
let mut ctx = MaybeUninit::<avm_codec_ctx_t>::uninit();
let res = avm_codec_dec_init_ver(
    ctx.as_mut_ptr(),
    iface,
    &cfg,
    0,
    AVM_DECODER_ABI_VERSION as i32,
);
// SAFETY: avm_codec_dec_init_ver fully initializes ctx on success.
let ctx = if res == AVM_CODEC_OK { ctx.assume_init() } else { ... };
```

For `cfg`, manually initialize the fields you set (`cfg.threads = t;`)
instead of relying on zero. Add a `// SAFETY:` comment auditing the
zero-tolerance contract for any field still left at zero.

---

### R6 — Always pass `&cfg` to `dec_init_ver` 🟠 MEDIUM

**File:** `src/decoder.rs:66`
**Why:** `if threads.is_some() { &cfg } else { ptr::null() }` saves zero
work (the cfg is allocated and zeroed regardless) and forwards a NULL
pointer to the C library when `threads` is None. libavm's init may
dereference `cfg`. **Not flagged by the author's audit.**

**How to apply:** Drop the ternary; always pass `&cfg`. The zero-init
defaults are fine.

---

### R7 — `avmdec.rs` plane indexing is unchecked 🟠 MEDIUM

**File:** `src/bin/avmdec.rs:131, 145`
**Why:** `plane[start + x * 2]` and `&plane[start..end]` are bounds-checked
panics on corrupted metadata. After R4, these are *probably* safe, but
defense-in-depth says use `.get(...)` with explicit error propagation.
Author's audit lists as **M-4 PARTIAL**.

**How to apply:** Replace direct indexing with `chunks_exact(stride)` walks
plus `.take(h)` and `.get(..row_bytes)`. Surface a `DecoderError::Malformed`
on `None`.

---

## SHOULD-FIX (audit-acknowledged debt)

### R8 — Add `// SAFETY:` comments to all 17 unsafe blocks in `decoder.rs`

**File:** `src/decoder.rs`
**Verification:** `cargo clippy --lib -- -W clippy::undocumented_unsafe_blocks`
prints exactly 17 warnings. Author's audit lists as **N-2**.
**How to apply:** Mechanical. The most important block is `plane()`
(line 301) — its `// SAFETY:` should enumerate all six preconditions
(`index < 3`, non-null pointer, non-negative stride, stride covers row
bytes, `len = stride*height` non-overflowing, `len <= img.sz`).

### R9 — Tests for the new validation paths in `plane()`

**File:** `tests/` (new file e.g. `plane_validation_test.rs`)
**Why:** Phase 1 added six validation branches in `plane()` and **none of
them are exercised by a test**. Audit acknowledges this gap.
**How to apply:** Build a mock `avm_image_t` (it is a repr(C) struct;
construct via `MaybeUninit::zeroed` and set the fields under test). Cases:
- negative `stride[0]` → `None`
- `stride < plane_w` → `None`
- `stride.checked_mul(height)` overflow (e.g. `stride = usize::MAX/2`,
  `height = 4`) → `None`
- `len > img.sz` when `sz > 0` → `None`
- valid frame round-trip → `Some` with the right length

### R10 — `Frame::img` → `NonNull<avm_image_t>`

**File:** `src/decoder.rs:230-232` (and all 11 accessors)
**Why:** Eliminates 11 `debug_assert!(!self.img.is_null())` calls (which
are debug-only and silent in release). `NonNull<T>` is `!Send + !Sync`,
so this also gets `Frame: !Send + !Sync` for free. Construct via
`NonNull::new(img_ptr)?` in `FrameIterator::next`.

### R11 — Wire up `ext_fb_manager` *or* delete it

**File:** `src/decoder.rs:37-47`
**Why:** Public trait `FrameBufferManager` + dead `ext_fb_manager` field +
`#[allow(dead_code)]` attribute is the worst of both worlds. Pick one:
- **Implement:** Take `Box<dyn FrameBufferManager + 'static>`, store in
  `ext_fb_manager`, register an internal `extern "C"` shim that recovers
  the trait object from `priv_`. Wrap shim body in `catch_unwind`.
- **Delete:** Remove the trait, remove the field, remove the attribute.

Recommend **delete** for now; implement when an actual user appears.

### R12 — Replace magic `0x800` with `AVM_IMG_FMT_HIGHBITDEPTH`

**Files:** `src/bin/avmdec.rs:118` and the new code added by R4 in
`src/decoder.rs`. Use the bindgen-generated constant.

### R13 — `Decoder::flush()` and call it at end of `avmdec.rs` loop

**Files:** `src/decoder.rs` (new method), `src/bin/avmdec.rs` (call site)
**Why:** Bitstreams with B-frame reordering will silently drop trailing
frames without an explicit drain. The C `avmdec` calls
`avm_codec_decode(ctx, NULL, 0, NULL)` at EOF for this reason.
**How to apply:**
```rust
pub fn flush(&mut self) -> Result<(), DecoderError> {
    unsafe {
        let res = avm_codec_decode(&mut self.ctx, ptr::null(), 0, ptr::null_mut());
        if res == avm_codec_err_t_AVM_CODEC_OK { Ok(()) } else { Err(DecoderError::DecodeFailed(res)) }
    }
}
```
Call after the input loop in `avmdec.rs`, then drain `get_frames()` once
more before printing the MD5.

### R14 — `BufWriter` around the output file in `avmdec.rs`

**File:** `src/bin/avmdec.rs:36`
**How to apply:** `out_file = Some(BufWriter::with_capacity(1 << 20, File::create(arg)?));`
Update the `Option<File>` type to `Option<BufWriter<File>>`. Per-row
syscalls drop from O(planes × rows) to O(buffer flushes).

---

## NICE-TO-HAVE

### R15 — `build.rs`: drop hardcoded GCC include path

**File:** `build.rs:21`
**How to apply:** Delete the `clang_arg("-I/usr/lib/gcc/...")` line and try
the build. If `<stddef.h>` resolution fails, run `clang -print-resource-dir`
from `build.rs` and pass that instead. Author's audit **L-3 UNRESOLVED**.

### R16 — `build.rs`: CMake bool defines

**File:** `build.rs:6-10`
**Why:** `define("CONFIG_AV2_ENCODER", "0")` passes the literal string `"0"`,
which is *truthy* in CMake's `if(DEFINED ...)` form. Only happens to be
treated as false because of CMake's `0`/`OFF`/`NO`/... allow-list for
falsy strings. **Not flagged by the author's audit.**
**How to apply:** Use `"OFF"` (and `"ON"` if anything ever needs to be
enabled).

### R17 — `build.rs`: rerun-if-changed

**File:** `build.rs`
**How to apply:**
```rust
println!("cargo:rerun-if-changed=build.rs");
println!("cargo:rerun-if-changed=avm/avm_decoder.h");
println!("cargo:rerun-if-changed=avm/avmdx.h");
// plus a glob over avm/CMakeLists.txt and the source tree if practical
```

### R18 — `clap` for `avmdec.rs` arg parsing

**File:** `src/bin/avmdec.rs:29-37`
**Why:** Currently `avmdec foo.ivf out.y4m --md5 also.y4m` silently drops
`also.y4m`; `avmdec foo.ivf --md` creates a file literally named `--md` in
cwd. No `--help`. Author's audit doesn't mention.

### R19 — `IvfReader::header` field encapsulation

**File:** `src/ivf.rs:18-19`
**Why:** Currently `pub header: IvfHeader` is mutable from outside.
**How to apply:** Make field private; add `pub fn header(&self) -> &IvfHeader`.

### R20 — Scope bindgen `allow`s to a private `mod sys`

**File:** `src/lib.rs:1-8`
**Why:** Currently `non_camel_case_types` etc. are crate-wide, silencing
naming lints on hand-written code. Wrap the bindings:
```rust
mod sys {
    #![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
pub use sys::{ /* curated re-exports */ };
```

### R21 — `Cargo.toml` metadata

**File:** `Cargo.toml`
**How to apply:** Add `description`, `license` (likely BSD-2-Clause to match
AVM upstream — verify), `repository`, `keywords = ["av2","video","decoder"]`,
`categories = ["multimedia::encoding"]`, `rust-version = "1.74"`
(or whatever the actual MSRV ends up being).

### R22 — `.gitignore` and repo hygiene

- Add `.gitignore` for `target/`, `out.y4m`, `out.yuv`, `*.y4m`, `*.yuv`,
  `Cargo.lock` (if treating as library), `**/*.pyc`, etc.
- Remove `out.y4m` and `out.yuv` from the working tree.
- Consider collapsing the five top-level audit `*.md` files
  (`SECURITY_AUDIT.md`, `HARDENING_SUMMARY.md`, `FINAL_AUDIT.md`,
  `TEST_STRATEGY.md`, `PORTING_PLAN.md`) into `docs/audits/` once the open
  items are closed.

### R23 — Fuzz target for `IvfReader` + `Decoder::decode`

**File:** `fuzz/fuzz_targets/ivf_decode.rs` (new, via `cargo-fuzz init`)
**Why:** Highest-value follow-up after R1 closes. Fuzzes both the Rust
parser (validates R1/R2) and the C decoder (libavm itself). Don't add to
mandatory CI; run periodically.

### R24 — Run unit tests under MIRI

**Why:** MIRI cannot cross the FFI boundary, but it will validate the
pure-Rust unsafe patterns in `Frame` math, IVF parsing, and lifetime
soundness. Requires updating the nightly toolchain (the installed
2024-03-20 nightly is too old to build current dependencies — see audit
section "Tooling Results").

---

## Out of scope (deferred indefinitely)

- **`extern "C-unwind"` / `catch_unwind` callback wrapper.** Theoretical
  until someone writes a panicking callback. Documented in `decoder.rs`
  drop impl and on `set_frame_buffer_functions`. No runtime enforcement.
- **AddressSanitizer in CI.** Requires toolchain update first.

---

## Suggested PR breakdown

**PR-A (must-fix, blocks any release on untrusted input):**
R1, R2, R3, R4, R5, R6, R7

**PR-B (audit-acknowledged debt cleanup):**
R8, R9, R10, R11, R12, R13, R14

**PR-C (hygiene):**
R15, R16, R17, R18, R19, R20, R21, R22

**PR-D (defense in depth, optional):**
R23, R24

PRs are ordered by priority. PR-A is the gating set; nothing in PR-B/C/D
should block PR-A from merging. After PR-A lands the crate is safe to feed
attacker-controlled IVF files into; before PR-A it is not.
