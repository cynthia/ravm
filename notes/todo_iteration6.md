# rustavm — TODO (Iteration 6 handover)

**Date:** 2026-04-08
**Status:** Iteration 5 (item #1: FFI hidden, type-safe enums) complete.
This iteration is the remaining "rust-friendly API" work — items 2–9 from
the prior critique.

`cargo build`, `cargo test`, `cargo clippy --all-targets --features bin`,
and `cargo clippy --lib -- -W clippy::undocumented_unsafe_blocks` are all
clean. **69 tests pass / 22 ignored / 0 fail.**

---

## Iteration 5 results (do not redo)

- New `src/format.rs` with `PixelFormat`, `ChromaSamplePosition`,
  `ColorRange`, `Subsampling` (all `#[non_exhaustive]`). 8 new unit tests.
- `mod sys` is now strictly private. New `pub mod ffi` re-exports a
  curated 19-symbol set for tests / advanced users.
- `Frame::format()` → `Option<PixelFormat>`; `format_raw()` added as
  escape hatch.
- `Frame::chroma_sample_position()` → `ChromaSamplePosition`.
- `Frame::color_range()` → `ColorRange`.
- Crate-level `//!` doc comment added to `lib.rs`.
- All consumers (avmdec, md5_verification_test, frame_buffer_test,
  examples/simple_decode) updated to the new API.

---

## Remaining work (priority order)

### A1 — `Frame::plane()` ergonomics + 16-bit view 🔴 HIGH

**Files:** `src/decoder.rs`, plus consumers in `src/bin/avmdec.rs` and
`tests/md5_verification_test.rs`.

**Why:** Today users do `plane[start + x * 2]` (HBD) or
`plane.chunks(stride).take(h)` (8-bit) by hand. Both `avmdec` and
`md5_verification_test` duplicate this pattern.

**How to apply:**

1. Add a `PlaneView<'a>` enum in `src/format.rs`:
   ```rust
   pub enum PlaneView<'a> {
       U8(&'a [u8]),
       U16(&'a [u16]),  // bytemuck::cast_slice with alignment check
   }
   ```
2. Add to `Frame`:
   ```rust
   pub fn plane_view(&self, idx: usize) -> Option<PlaneView<'_>>;
   pub fn rows(&self, idx: usize) -> impl Iterator<Item = &[u8]>;
   ```
   `rows()` does the `chunks(stride).take(h).map(|r| &r[..row_bytes])`
   walk once, correctly, with the HBD bytes-per-sample math built in.
3. Replace the duplicated row loops in `avmdec.rs` and `frame_md5` in
   `md5_verification_test.rs` with `frame.rows(i)` calls.

**Note:** `bytemuck` is a tiny dep with no transitive baggage; gate it
behind a `bytemuck` feature if minimizing deps matters.

**Effort:** ~half day. Touches 3 files but eliminates >100 lines of
hand-written stride/HBD logic.

---

### A2 — `DecoderError` proper variants + `thiserror` 🟠 MEDIUM

**File:** `src/decoder.rs`

**Why:** `DecoderError::CodecError(avm_codec_err_t)` currently displays
as a bare integer. Users have no way to distinguish "bitstream corrupt"
from "out of memory" from "feature unsupported" without reading
`avm/avm_codec.h`.

**How to apply:**

1. Add `thiserror = "1"` to `[dependencies]`.
2. Replace `DecoderError` with one variant per `avm_codec_err_t_*`
   constant in the bindings:
   ```rust
   #[derive(Debug, thiserror::Error)]
   #[non_exhaustive]
   pub enum DecoderError {
       #[error("codec initialization failed")]
       InitFailed,
       #[error("decoder needs more input data")]
       NeedMoreData,
       #[error("bitstream is malformed")]
       BadBitstream,
       #[error("requested feature is unsupported")]
       Unsupported,
       #[error("codec internal allocation failed")]
       OutOfMemory,
       #[error("invalid parameter")]
       InvalidParam,
       #[error("ABI mismatch with libavm")]
       AbiVersionMismatch,
       #[error("unrecognized codec error code: {0}")]
       Other(u32),
   }
   impl DecoderError {
       pub(crate) fn from_raw(code: avm_codec_err_t) -> Self { ... }
   }
   ```
3. Optionally include the `avm_codec_error_to_string()` /
   `avm_codec_error_detail()` C strings via a `source()`-like method.
4. Update all `Err(DecoderError::CodecError(res))` and similar to call
   `Err(DecoderError::from_raw(res))`.

**Effort:** ~2 hours.

---

### A3 — `IvfReader: Iterator<Item = io::Result<IvfFrame>>` 🟠 MEDIUM

**File:** `src/ivf.rs`

**Why:** Today: `while let Some(pkt) = reader.next_frame()? { ... }`.
Idiomatic: `for pkt in reader { let pkt = pkt?; ... }`.

**How to apply:**

```rust
impl<R: Read> Iterator for IvfReader<R> {
    type Item = io::Result<IvfFrame>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.next_frame() {
            Ok(Some(f)) => Some(Ok(f)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}
```

Also add `IvfReader::open<P: AsRef<Path>>(path: P) -> io::Result<Self>`
to save callers the `File::open` + `BufReader::new` boilerplate.

Update consumers (`avmdec.rs`, `md5_verification_test.rs`, fuzz target,
example) to use `for` loops.

**Effort:** ~1 hour.

---

### A4 — `DecoderBuilder` 🟠 MEDIUM

**File:** `src/decoder.rs`

**Why:** `Decoder::with_config(Some(4))` is OK for one parameter; it
becomes ugly when item A6 reintroduces `FrameBufferManager`. Switch to
a builder now so the API doesn't churn later.

**How to apply:**

```rust
pub struct DecoderBuilder {
    threads: Option<u32>,
    fb_manager: Option<Box<dyn FrameBufferManager>>,
}

impl DecoderBuilder {
    pub fn threads(mut self, n: u32) -> Self { self.threads = Some(n); self }
    pub fn frame_buffer_manager(mut self, m: impl FrameBufferManager + 'static) -> Self { ... }
    pub fn build(self) -> Result<Decoder, DecoderError> { ... }
}

impl Decoder {
    pub fn builder() -> DecoderBuilder { DecoderBuilder { threads: None, fb_manager: None } }
}
```

Keep `Decoder::new()` and deprecate `Decoder::with_config()` with
`#[deprecated(note = "use Decoder::builder().threads(n).build()")]` for
one release before removal.

**Effort:** ~2 hours including deprecation handling.

---

### A5 — `OwnedFrame` for cross-borrow / cross-thread 🟠 MEDIUM

**Files:** `src/decoder.rs` (or new `src/frame.rs`)

**Why:** `Frame<'_>` borrows the decoder. To send a decoded frame to
another thread or hold it across `decoder.decode()`, users must copy
each plane manually. Standard idiom is a `to_owned()` snapshot.

**How to apply:**

```rust
pub struct OwnedFrame {
    pub width: u32,
    pub height: u32,
    pub bit_depth: u32,
    pub format: Option<PixelFormat>,
    pub color_range: ColorRange,
    pub chroma_sample_position: ChromaSamplePosition,
    pub planes: [Vec<u8>; 3],   // index 1/2 empty for monochrome
    pub strides: [usize; 3],
}

impl Frame<'_> {
    pub fn to_owned(&self) -> OwnedFrame { ... }
}
```

`OwnedFrame` is `Send + Sync` for free since it owns its data. Pairs
naturally with A1's `rows()` API for the per-plane copy.

**Effort:** ~2 hours.

---

### A6 — Reintroduce safe `FrameBufferManager` trait 🟠 MEDIUM

**File:** `src/decoder.rs`

**Why:** The trait was deleted in iteration 1. The only `unsafe fn` on
the public API today is `set_frame_buffer_functions`. Replacing it with
a typed trait eliminates the last public unsafe surface.

**How to apply:**

```rust
pub trait FrameBufferManager: Send {
    /// Allocate a buffer of at least `min_size` bytes.  Return the
    /// buffer pointer (or null on failure) and an opaque token the
    /// release callback can use to identify it.
    fn allocate(&mut self, min_size: usize) -> Option<(NonNull<u8>, usize, *mut ())>;
    /// Release a buffer previously returned by `allocate`.  `token` is
    /// the third tuple element from the matching `allocate` call.
    fn release(&mut self, token: *mut ());
}

impl Decoder {
    pub fn set_frame_buffer_manager<M: FrameBufferManager + 'static>(
        &mut self,
        manager: M,
    ) -> Result<(), DecoderError> {
        // Box the manager, store in self, register an internal
        // extern "C" shim that recovers it from priv_, wraps the trait
        // calls in catch_unwind, and translates to error codes.
    }
}
```

Implementation requires:
- A `Box<dyn FrameBufferManager>` field on `Decoder` (its corpse from
  iteration 1 existed at one point).
- Two `extern "C"` shim functions that read `priv_` as
  `*mut Box<dyn FrameBufferManager>` and forward to the trait methods.
- `std::panic::catch_unwind` around each trait call so a Rust panic in
  user code becomes a clean "-1" return to libavm instead of a process
  abort.

The current `unsafe fn set_frame_buffer_functions` can stay for one
release with `#[deprecated]` then go.

**Effort:** ~half day. The hard part is the catch_unwind + lifetime
plumbing; the `frame_buffer_test.rs` integration test already exercises
the equivalent C-level code, so behavior parity is testable.

---

### A7 — Streaming convenience helper 🟡 LOW-MEDIUM

**File:** new `src/streaming.rs` or in `decoder.rs`

**Why:** The CLI's main loop is ~50 lines that should be a four-line
library call.

**How to apply:**

```rust
pub fn decode_ivf<R, F>(reader: R, mut sink: F) -> Result<usize, DecoderError>
where
    R: std::io::Read,
    F: FnMut(Frame<'_>),
{
    let mut ivf = IvfReader::new(reader)?;
    let mut decoder = Decoder::new()?;
    let mut count = 0;
    for pkt in &mut ivf {
        decoder.decode(&pkt?.data)?;
        for frame in decoder.get_frames() { sink(frame); count += 1; }
    }
    decoder.flush()?;
    for frame in decoder.get_frames() { sink(frame); count += 1; }
    Ok(count)
}
```

Depends on A3 (Iterator impl) and A2 (proper error type that absorbs
`io::Error`).

**Effort:** ~1 hour.

---

### A8 — Standard trait impls 🟢 LOW

**Files:** `src/decoder.rs`

- `Decoder`: manual `Debug` (`f.debug_struct("Decoder").finish_non_exhaustive()`).
- `Frame`: manual `Debug` printing width/height/format/bit_depth.
- `StreamInfo`: derive `Debug, Clone, Copy, PartialEq, Eq`.
- `From<io::Error> for DecoderError` (after A2 lands).

**Effort:** ~30 minutes total.

---

### A9 — Polish 🟢 LOW

- **`examples/simple_decode.rs`:** accept input path as a CLI arg
  instead of hardcoded `../avm/out/test.ivf`. Two-line change.
- **`#[must_use]`** on `Decoder::builder()`, `Frame` accessors, and
  `Result`-returning constructors.
- **More examples:** "decode first frame to PNG via the `image` crate"
  as a separate `examples/decode_to_png.rs` to demonstrate `OwnedFrame`
  + the `rows()` iterator.
- **`avmdec` `--threads N`** flag wired to `Decoder::builder().threads(n)`.

**Effort:** ~1 hour total.

---

## Carried from prior iterations

- **N7 — Cargo.toml `repository` field.** Still blocked on canonical
  URL confirmation.

---

## Suggested PR breakdown

| PR | Items | Effort |
|---|---|---|
| `api/plane-view-rows` | A1 | half day |
| `api/error-types` | A2, A8 (`From<io::Error>`) | ~3h |
| `api/ivf-iterator` | A3, A7 | ~2h |
| `api/builder-and-fb-manager` | A4, A6, A8 (Decoder Debug) | ~1 day |
| `api/owned-frame` | A5, A8 (Frame Debug) | ~3h |
| `api/polish` | A9 | ~1h |

**Total for full Rust-friendly API: ~3 days of focused work.**

---

## Out of scope (carried, deferred indefinitely)

- `extern "C-unwind"` callbacks — proven impossible in iteration 4 without
  re-introducing a transmute.
- Async wrapper — wrong abstraction for CPU-bound video decode.
- `no_std` — libavm needs heap.
- Miri / ASAN in CI — needs nightly toolchain refresh.
- FourCC allow-list in `IvfReader` — add only on concrete user request.
