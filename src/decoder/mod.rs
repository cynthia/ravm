#![deny(unsafe_op_in_unsafe_fn)]

pub(crate) mod core;
pub(crate) mod entropy;
pub(crate) mod executor;
pub(crate) mod frame_buffer;
pub(crate) mod intra;
pub(crate) mod kernels;
pub(crate) mod block_info;
pub(crate) mod partition;
pub(crate) mod quant;
pub(crate) mod symbols;
pub(crate) mod transform;

use crate::backend::BackendKind;
use crate::backend::libavm::LibavmDecoder;
use crate::backend::rust::RustDecoder;
use crate::bitstream::{FrameHeaderInfo, FramePacketKind, SequenceHeader};
use crate::format::{ChromaSamplePosition, ColorRange, PixelFormat, PlaneView};
use std::fmt;
use crate::sys::{
    avm_codec_err_t,
    avm_codec_err_t_AVM_CODEC_ABI_MISMATCH, avm_codec_err_t_AVM_CODEC_CORRUPT_FRAME,
    avm_codec_err_t_AVM_CODEC_ERROR, avm_codec_err_t_AVM_CODEC_INCAPABLE,
    avm_codec_err_t_AVM_CODEC_INVALID_PARAM, avm_codec_err_t_AVM_CODEC_MEM_ERROR,
    avm_codec_err_t_AVM_CODEC_OK, avm_codec_err_t_AVM_CODEC_UNSUP_BITSTREAM,
    avm_codec_err_t_AVM_CODEC_UNSUP_FEATURE, avm_codec_frame_buffer_t, avm_codec_iter_t,
    avm_image_t, avm_img_fmt,
    AVM_IMG_FMT_HIGHBITDEPTH,
};
use std::marker::PhantomData;
use std::os::raw::{c_int, c_void};
use std::ptr::NonNull;
use std::slice;

/// One specific libavm error code, modeled as a Rust enum.
///
/// Mirrors the `avm_codec_err_t_*` constants from the C header.  Use
/// [`ErrorKind::from_raw`] to convert from the raw integer.  Unknown codes
/// are preserved via [`ErrorKind::Other`] so future libavm releases that add
/// new error variants degrade gracefully.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, thiserror::Error)]
#[non_exhaustive]
pub enum ErrorKind {
    /// `AVM_CODEC_ERROR` — generic, unspecified error.
    #[error("libavm reported a generic error")]
    Generic,
    /// `AVM_CODEC_MEM_ERROR` — internal allocation failed.
    #[error("libavm allocation failed")]
    OutOfMemory,
    /// `AVM_CODEC_ABI_MISMATCH` — wrapper compiled against a different ABI
    /// than the loaded libavm.
    #[error("libavm ABI version mismatch")]
    AbiMismatch,
    /// `AVM_CODEC_INCAPABLE` — the decoder cannot perform the operation.
    #[error("decoder is incapable of this operation")]
    Incapable,
    /// `AVM_CODEC_UNSUP_BITSTREAM` — bitstream profile/level is unsupported.
    #[error("unsupported bitstream profile or level")]
    UnsupportedBitstream,
    /// `AVM_CODEC_UNSUP_FEATURE` — bitstream uses an unsupported feature.
    #[error("unsupported bitstream feature")]
    UnsupportedFeature,
    /// `AVM_CODEC_CORRUPT_FRAME` — the input is malformed.
    #[error("corrupt bitstream frame")]
    CorruptFrame,
    /// `AVM_CODEC_INVALID_PARAM` — an invalid argument was passed.
    #[error("invalid parameter")]
    InvalidParam,
    /// libavm returned a code unknown to this version of `rustavm`.
    #[error("libavm returned unknown error code {0}")]
    Other(avm_codec_err_t),
}

impl ErrorKind {
    /// Convert a raw libavm `avm_codec_err_t` into a typed `ErrorKind`.
    ///
    /// Returns `None` for `AVM_CODEC_OK` (i.e. no error).  All non-zero
    /// codes produce `Some(_)`, with unrecognized values mapped to
    /// [`ErrorKind::Other`].
    pub fn from_raw(code: avm_codec_err_t) -> Option<Self> {
        if code == avm_codec_err_t_AVM_CODEC_OK {
            return None;
        }
        let kind = match code {
            x if x == avm_codec_err_t_AVM_CODEC_ERROR => Self::Generic,
            x if x == avm_codec_err_t_AVM_CODEC_MEM_ERROR => Self::OutOfMemory,
            x if x == avm_codec_err_t_AVM_CODEC_ABI_MISMATCH => Self::AbiMismatch,
            x if x == avm_codec_err_t_AVM_CODEC_INCAPABLE => Self::Incapable,
            x if x == avm_codec_err_t_AVM_CODEC_UNSUP_BITSTREAM => Self::UnsupportedBitstream,
            x if x == avm_codec_err_t_AVM_CODEC_UNSUP_FEATURE => Self::UnsupportedFeature,
            x if x == avm_codec_err_t_AVM_CODEC_CORRUPT_FRAME => Self::CorruptFrame,
            x if x == avm_codec_err_t_AVM_CODEC_INVALID_PARAM => Self::InvalidParam,
            other => Self::Other(other),
        };
        Some(kind)
    }
}

/// Errors returned by the safe [`Decoder`] API.
///
/// Each variant carries the [`ErrorKind`] returned by libavm and identifies
/// the operation that produced the error so callers can react appropriately
/// (e.g. retry decode after `OutOfMemory`, abort after `AbiMismatch`).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DecoderError {
    /// `Decoder::new` / `Decoder::with_config` / `DecoderBuilder::build` failed.
    #[error("decoder initialization failed: {0}")]
    Init(ErrorKind),
    /// `Decoder::decode` rejected the input.
    #[error("decode failed: {0}")]
    Decode(ErrorKind),
    /// `Decoder::flush` failed.
    #[error("flush failed: {0}")]
    Flush(ErrorKind),
    /// `Decoder::get_stream_info` failed.
    #[error("stream info query failed: {0}")]
    StreamInfo(ErrorKind),
    /// `Decoder::set_frame_buffer_functions` rejected the request.
    #[error("set_frame_buffer_functions failed: {0}")]
    SetFrameBufferFunctions(ErrorKind),
    /// Pure-Rust packet parsing rejected the compressed input.
    #[error("bitstream parse failed: {0}")]
    Parse(&'static str),
    /// The selected backend recognized the request but the capability is not implemented yet.
    #[error("decoder feature not implemented: {0}")]
    Unimplemented(&'static str),
    /// The selected backend exists in the API surface but is not implemented yet.
    #[error("decoder backend `{0}` is not available in this build")]
    BackendUnavailable(BackendKind),
    /// I/O error surfaced by higher-level helpers that normalize decode output.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

pub struct Decoder {
    inner: DecoderInner,
}

/// High-level parser/decode event surfaced by a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeEvent {
    /// A sequence header was parsed successfully.
    SequenceHeader(SequenceHeader),
    /// A frame header or frame-bearing packet yielded header semantics.
    FrameHeader(FrameHeaderInfo),
}

/// Observable parser/decode progress for the active backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeProgress {
    pub backend: BackendKind,
    pub packets_parsed: Option<usize>,
    pub obus_parsed: Option<usize>,
    pub frame_packets_seen: Option<usize>,
    pub sequence_header: Option<SequenceHeader>,
    pub stream_info: Option<StreamInfo>,
    pub last_frame_packet_kind: Option<FramePacketKind>,
    pub last_frame_header: Option<FrameHeaderInfo>,
    pub last_event: Option<DecodeEvent>,
    pub recent_events: [Option<DecodeEvent>; 4],
}

impl fmt::Debug for Decoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.inner {
            DecoderInner::Libavm(inner) => f.debug_tuple("Decoder").field(inner).finish(),
            DecoderInner::Rust(inner) => f.debug_tuple("Decoder").field(inner).finish(),
        }
    }
}

/// Per-buffer record returned from [`FrameBufferManager::allocate`] and
/// passed back to [`FrameBufferManager::release`].
///
/// `data` and `len` describe the buffer; `token` is opaque to libavm and
/// to the wrapper — managers typically use it as an index, slot ID, or
/// reinterpret it as a pointer to per-buffer metadata.
#[derive(Debug, Clone, Copy)]
pub struct FrameBuffer {
    /// Pointer to the start of the buffer.  Must remain valid until the
    /// matching [`FrameBufferManager::release`] call.
    pub data: NonNull<u8>,
    /// Length of the buffer in bytes.  Must be `>= min_size` from
    /// [`FrameBufferManager::allocate`].
    pub len: usize,
    /// Opaque per-buffer token that libavm hands back unchanged in the
    /// matching `release` call.
    pub token: usize,
}

// SAFETY: `FrameBuffer` is metadata only.  The pointer aliases storage that
// the manager owns and is responsible for keeping alive across threads
// according to the trait contract.
unsafe impl Send for FrameBuffer {}

/// User-implementable trait for supplying decoded-frame storage to libavm.
///
/// Replaces the older `unsafe fn set_frame_buffer_functions` API.  Implement
/// this on a type that owns a pool of `Vec<u8>`s (or `mmap` regions, etc.),
/// install it via [`Decoder::set_frame_buffer_manager`], and the safe
/// wrapper installs panic-catching `extern "C"` shims that drive the
/// trait.
///
/// Implementations must be `Send` because the manager is owned by the
/// `Decoder` and may be moved across threads when the decoder is
/// (although the decoder itself is `!Send`, future builders may relax that
/// — keeping the trait bound `Send` makes the API forward-compatible).
pub trait FrameBufferManager: Send {
    /// Allocate a buffer of at least `min_size` bytes.  Return `None` to
    /// signal allocation failure (libavm will report `OutOfMemory`).
    ///
    /// The returned [`FrameBuffer`] must remain valid (data pointer alive,
    /// length unchanged) until the matching [`Self::release`] call.
    fn allocate(&mut self, min_size: usize) -> Option<FrameBuffer>;

    /// Release a buffer previously returned by [`Self::allocate`].  After
    /// this call returns, the manager may free or recycle the storage.
    fn release(&mut self, buffer: FrameBuffer);
}

enum DecoderInner {
    Libavm(LibavmDecoder),
    Rust(Box<RustDecoder>),
}

/// Fluent builder for configuring a [`Decoder`] before construction.
///
/// # Example
///
/// ```no_run
/// use rustavm::decoder::Decoder;
/// let decoder = Decoder::builder().threads(4).build()?;
/// # Ok::<(), rustavm::decoder::DecoderError>(())
/// ```
#[derive(Debug, Default, Clone)]
#[must_use = "DecoderBuilder does nothing until `.build()` is called"]
pub struct DecoderBuilder {
    threads: Option<u32>,
    backend: BackendKind,
}

impl DecoderBuilder {
    /// Create a new builder with default settings.
    pub fn new() -> Self {
        Self {
            threads: None,
            backend: BackendKind::Libavm,
        }
    }

    /// Set the worker thread count for libavm's internal thread pool.
    ///
    /// `0` lets the codec choose; non-zero values cap the pool size.
    pub fn threads(mut self, n: u32) -> Self {
        self.threads = Some(n);
        self
    }

    /// Select the decode backend.
    ///
    /// [`BackendKind::Libavm`] is the current production path.
    /// [`BackendKind::Rust`] enables the in-tree pure-Rust decoder, which
    /// currently implements the M0 walking-skeleton subset.
    pub fn backend(mut self, backend: BackendKind) -> Self {
        self.backend = backend;
        self
    }

    /// Construct the [`Decoder`].
    pub fn build(self) -> Result<Decoder, DecoderError> {
        match self.backend {
            BackendKind::Libavm => Ok(Decoder {
                inner: DecoderInner::Libavm(LibavmDecoder::new(self.threads)?),
            }),
            BackendKind::Rust => Ok(Decoder {
                inner: DecoderInner::Rust(Box::new(RustDecoder::new(self.threads)?)),
            }),
        }
    }
}

impl Decoder {
    /// Create a decoder with default configuration (no threading hint).
    ///
    /// Equivalent to `Decoder::builder().build()`.
    pub fn new() -> Result<Self, DecoderError> {
        DecoderBuilder::new().build()
    }

    /// Returns a [`DecoderBuilder`] for configuring optional parameters.
    pub fn builder() -> DecoderBuilder {
        DecoderBuilder::new()
    }

    /// Returns the backend in use by this decoder.
    pub const fn backend_kind(&self) -> BackendKind {
        match self.inner {
            DecoderInner::Libavm(_) => BackendKind::Libavm,
            DecoderInner::Rust(_) => BackendKind::Rust,
        }
    }

    /// Returns backend-specific parser/decode progress.
    pub fn progress(&self) -> DecodeProgress {
        match &self.inner {
            DecoderInner::Libavm(inner) => inner.progress(),
            DecoderInner::Rust(inner) => inner.progress(),
        }
    }

    /// Create a decoder with an optional explicit thread count.
    ///
    /// Prefer [`Decoder::builder`] for new code.
    #[deprecated(since = "0.2.0", note = "use `Decoder::builder().threads(n).build()` instead")]
    pub fn with_config(threads: Option<u32>) -> Result<Self, DecoderError> {
        let mut builder = DecoderBuilder::new();
        if let Some(t) = threads {
            builder = builder.threads(t);
        }
        builder.build()
    }

    /// Submit compressed data to the decoder.
    ///
    /// The data may contain zero or more frames; call [`get_frames`] after
    /// this returns to retrieve any decoded output.
    ///
    /// # Data lifetime
    ///
    /// `avm_codec_decode` processes `data` **synchronously**: it parses OBUs inline and does not
    /// retain the pointer after the call returns.  The `&[u8]` borrow is therefore sound — the
    /// caller's buffer may be reused or dropped immediately after this call completes.
    /// (Verified in `avm_decoder.h`: the function takes `const uint8_t *data` with no out-parameter
    /// to retain the pointer; AOM heritage confirms data is not aliased past the call.)
    pub fn decode(&mut self, data: &[u8]) -> Result<(), DecoderError> {
        match &mut self.inner {
            DecoderInner::Libavm(inner) => inner.decode(data),
            DecoderInner::Rust(inner) => inner.decode(data),
        }
    }

    /// Flush the decoder, signaling end-of-stream.
    ///
    /// After the last packet has been decoded, call `flush()` followed by
    /// `get_frames()` to retrieve any buffered frames (e.g. due to B-frame
    /// reordering). Equivalent to the C idiom `avm_codec_decode(ctx, NULL, 0, NULL)`.
    pub fn flush(&mut self) -> Result<(), DecoderError> {
        match &mut self.inner {
            DecoderInner::Libavm(inner) => inner.flush(),
            DecoderInner::Rust(inner) => inner.flush(),
        }
    }

    /// Returns an iterator over decoded frames available after the last `decode()` call.
    ///
    /// # Double-call safety
    ///
    /// Calling `get_frames()` a second time while frames from the first call are still alive is
    /// statically impossible: `get_frames()` takes `&mut self`, and each `Frame<'a>` carries the
    /// same `'a` lifetime as the mutable borrow.  The borrow checker therefore prevents concurrent
    /// iterators, making use-after-next-decode impossible at compile time.
    pub fn get_frames(&mut self) -> FrameIterator<'_> {
        FrameIterator {
            decoder: self,
            iter: std::ptr::null(),
        }
    }

    pub fn get_stream_info(&mut self) -> Result<StreamInfo, DecoderError> {
        match &mut self.inner {
            DecoderInner::Libavm(inner) => inner.get_stream_info(),
            DecoderInner::Rust(inner) => inner.get_stream_info(),
        }
    }

    /// Sets external frame buffer functions.
    ///
    /// The callback parameter types use `c_int` to exactly match `avm_get_frame_buffer_cb_fn_t`
    /// and `avm_release_frame_buffer_cb_fn_t` from `avm/avm_frame_buffer.h`.  This avoids the
    /// `transmute` that was previously needed to paper over the `i32` / `c_int` alias mismatch.
    ///
    /// # Safety
    ///
    /// - `priv_` is stored by the C library and passed back to each callback invocation.  The
    ///   caller must ensure the data it points to outlives the `Decoder`.  There is no Rust
    ///   lifetime tie — the caller bears full responsibility for this invariant.
    /// - If a callback panics, the panic cannot unwind through the surrounding `extern "C"`
    ///   frame: since Rust 1.71 the runtime aborts the process instead.  This is defined
    ///   behavior but still unfortunate; wrap callback bodies in `std::panic::catch_unwind`
    ///   if a panic should be recoverable.  (We cannot use `extern "C-unwind"` here because
    ///   the bindgen-generated callback typedef in `avm_frame_buffer.h` is `extern "C"` and
    ///   the two function-pointer types do not coerce.)
    pub unsafe fn set_frame_buffer_functions(
        &mut self,
        get_fb: unsafe extern "C" fn(
            priv_: *mut c_void,
            min_size: usize,
            fb: *mut avm_codec_frame_buffer_t,
        ) -> c_int,
        release_fb: unsafe extern "C" fn(
            priv_: *mut c_void,
            fb: *mut avm_codec_frame_buffer_t,
        ) -> c_int,
        priv_: *mut c_void,
    ) -> Result<(), DecoderError> {
        match &mut self.inner {
            DecoderInner::Libavm(inner) => unsafe {
                inner.set_frame_buffer_functions(get_fb, release_fb, priv_)
            },
            DecoderInner::Rust(inner) => unsafe {
                inner.set_frame_buffer_functions(get_fb, release_fb, priv_)
            },
        }
    }

    /// Install a safe [`FrameBufferManager`] on this decoder.
    ///
    /// The manager takes ownership of decoded frame storage allocation:
    /// libavm calls back into [`FrameBufferManager::allocate`] when it
    /// needs storage and [`FrameBufferManager::release`] when a frame is
    /// no longer referenced.  Panics in the trait methods are caught
    /// (`std::panic::catch_unwind`) and translated to error returns so
    /// they cannot unwind through the C frame.
    ///
    /// Replacing an existing manager is allowed: the previous manager is
    /// dropped after libavm has switched to the new shim registration.
    pub fn set_frame_buffer_manager<M: FrameBufferManager + 'static>(
        &mut self,
        manager: M,
    ) -> Result<(), DecoderError> {
        match &mut self.inner {
            DecoderInner::Libavm(inner) => inner.set_frame_buffer_manager(manager),
            DecoderInner::Rust(inner) => inner.set_frame_buffer_manager(manager),
        }
    }
}

pub struct FrameIterator<'a> {
    decoder: &'a mut Decoder,
    iter: avm_codec_iter_t,
}

impl<'a> Iterator for FrameIterator<'a> {
    type Item = Frame<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        // SAFETY: `self.decoder.ctx` is valid (Decoder is alive via `&mut` borrow).
        // `self.iter` is initialized to null on construction and advanced internally
        // by the C library. The returned image pointer is valid until the next
        // `decode()` call, which is prevented by Frame's `'a` lifetime tying it to
        // the `&mut Decoder` borrow. We cast the `*const` return to `*mut` for
        // `NonNull::new`; no mutation occurs through this pointer.
        match &mut self.decoder.inner {
            DecoderInner::Libavm(inner) => inner.get_frame(&mut self.iter).map(|img| Frame {
                img,
                _marker: PhantomData,
            }),
            DecoderInner::Rust(inner) => inner.get_frame(&mut self.iter).map(|img| Frame {
                img,
                _marker: PhantomData,
            }),
        }
    }
}

/// Bitstream-level information reported by the decoder once at least one
/// packet has been parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamInfo {
    pub width: u32,
    pub height: u32,
    pub is_kf: bool,
    pub number_tlayers: u32,
    pub number_mlayers: u32,
    pub number_xlayers: u32,
}

pub struct Frame<'a> {
    img: NonNull<avm_image_t>,
    _marker: PhantomData<&'a Decoder>,
}

impl<'a> fmt::Debug for Frame<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frame")
            .field("width", &self.width())
            .field("height", &self.height())
            .field("bit_depth", &self.bit_depth())
            .field("format", &self.format())
            .field("monochrome", &self.monochrome())
            .finish_non_exhaustive()
    }
}

impl<'a> Frame<'a> {
    #[must_use]
    pub fn width(&self) -> u32 {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        unsafe { (*self.img.as_ptr()).d_w }
    }

    pub fn height(&self) -> u32 {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        unsafe { (*self.img.as_ptr()).d_h }
    }

    pub fn bit_depth(&self) -> u32 {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        unsafe { (*self.img.as_ptr()).bit_depth }
    }

    /// Returns the type-safe pixel format of this frame, or `None` if the
    /// underlying libavm value is not one recognized by [`PixelFormat`].
    ///
    /// Use [`Frame::format_raw`] for the underlying integer if you need to
    /// handle a format this crate does not yet model.
    pub fn format(&self) -> Option<PixelFormat> {
        PixelFormat::from_raw(self.format_raw())
    }

    /// Returns the raw libavm `avm_img_fmt` integer for this frame.
    pub fn format_raw(&self) -> crate::sys::avm_img_fmt {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        unsafe { (*self.img.as_ptr()).fmt }
    }

    pub fn monochrome(&self) -> bool {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        unsafe { (*self.img.as_ptr()).monochrome != 0 }
    }

    /// Returns the position of chroma samples relative to the luma grid.
    pub fn chroma_sample_position(&self) -> ChromaSamplePosition {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        ChromaSamplePosition::from_raw(unsafe { (*self.img.as_ptr()).csp })
    }

    /// Returns the sample value range — limited or full.
    pub fn color_range(&self) -> ColorRange {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        ColorRange::from_raw(unsafe { (*self.img.as_ptr()).range })
    }

    /// Returns a byte slice covering the full (stride × height) storage of the given plane.
    ///
    /// Returns `None` if:
    /// - `index >= 3`
    /// - the plane pointer is null (e.g. monochrome image, chroma planes are null)
    /// - `stride` is negative (vertically-flipped image via `avm_img_flip`; unflip first)
    /// - `stride < plane_width(index) * bytes_per_sample` (stride narrower than active pixel data — corrupted frame)
    /// - `stride * height` overflows `usize` (adversarial / corrupted frame dimensions)
    /// - `stride * height > img.sz` when `sz > 0` (slice would exceed total image allocation)
    pub fn plane(&self, index: usize) -> Option<&[u8]> {
        if index >= 3 {
            return None;
        }
        // SAFETY: `self.img` is non-null (NonNull invariant) and valid for `'a`.
        // `index < 3` is checked above, so `planes[index]` and `stride[index]` are
        // in-bounds. The plane pointer is checked for null (e.g. monochrome chroma).
        // `stride` is checked non-negative. `stride >= row_bytes` ensures each row
        // fits. `stride * height` is computed via `checked_mul` to prevent overflow.
        // When `img.sz > 0`, `len <= sz` is verified so the slice does not exceed the
        // total image allocation. The returned slice borrows `Frame<'a>`, which holds
        // a `&'a mut Decoder` borrow — this prevents `decode()` from invalidating the
        // pointer while the slice is live.
        unsafe {
            let img = self.img.as_ptr();
            let plane_ptr = (*img).planes[index];
            if plane_ptr.is_null() {
                return None;
            }
            // stride[index] is a signed C int; negative means the image was vertically flipped
            // via avm_img_flip().  We do not support flipped images in this API.
            let raw_stride = (*img).stride[index];
            if raw_stride < 0 {
                return None;
            }
            let stride = raw_stride as usize;

            // Sanity: stride (in bytes) must cover at least the active pixel row.
            // For high-bitdepth formats each pixel is 2 bytes.
            let plane_w = self.plane_width(index);
            let bps: usize = if ((*img).fmt & AVM_IMG_FMT_HIGHBITDEPTH) != 0 { 2 } else { 1 };
            let row_bytes = plane_w.checked_mul(bps)?;
            if stride < row_bytes {
                return None;
            }

            let height = self.height_for_plane(index);
            // checked_mul prevents usize overflow on adversarial or corrupted inputs.
            let len = stride.checked_mul(height)?;

            // img.sz is the total allocation size for img_data, set by the C allocator.
            // When non-zero it is a hard upper bound: no single-plane view can exceed the
            // entire image buffer.  (sz == 0 means "externally managed / unknown size".)
            let sz = (*img).sz;
            if sz > 0 && len > sz {
                return None;
            }

            Some(slice::from_raw_parts(plane_ptr, len))
        }
    }

    /// Returns the stride (in bytes) for the given plane.
    ///
    /// Returns 0 if `index >= 3` or the stride is negative (vertically-flipped image).
    pub fn stride(&self, index: usize) -> usize {
        if index >= 3 {
            return 0;
        }
        // SAFETY: `self.img` is non-null (NonNull invariant) and valid for `'a`.
        // `index < 3` is checked above, so `stride[index]` is in-bounds.
        unsafe {
            let s = (*self.img.as_ptr()).stride[index];
            // Negative stride means the image was flipped via avm_img_flip(); return 0 rather
            // than silently wrapping to a huge usize.
            if s < 0 {
                0
            } else {
                s as usize
            }
        }
    }

    /// Returns the active pixel width of the given plane, or 0 if `index >= 3`.
    pub fn plane_width(&self, index: usize) -> usize {
        if index >= 3 {
            return 0;
        }
        // SAFETY: `self.img` is non-null (NonNull invariant) and valid for `'a`.
        // `index < 3` is checked above. `x_chroma_shift` is clamped to 31 to prevent
        // shift-overflow panics.
        unsafe {
            let w = (*self.img.as_ptr()).d_w as usize;
            if index == 0 {
                w
            } else {
                // x_chroma_shift is an unsigned C int.  Shifting usize by a value >= usize::BITS
                // panics in debug and produces UB in release; clamp to 31 (max meaningful value
                // for standard subsampling is 1).
                let shift = (*self.img.as_ptr()).x_chroma_shift.min(31) as usize;
                (w + (1 << shift) - 1) >> shift
            }
        }
    }

    /// Returns the active pixel height of the given plane, or 0 if `index >= 3`.
    pub fn chroma_plane_height(&self, index: usize) -> usize {
        if index >= 3 {
            return 0;
        }
        // SAFETY: `self.img` is non-null (NonNull invariant) and valid for `'a`.
        // `index < 3` is checked above. `y_chroma_shift` is clamped to 31 to prevent
        // shift-overflow panics.
        unsafe {
            let h = (*self.img.as_ptr()).d_h as usize;
            if index == 0 {
                h
            } else {
                // y_chroma_shift is an unsigned C int; same clamping rationale as x_chroma_shift.
                let shift = (*self.img.as_ptr()).y_chroma_shift.min(31) as usize;
                (h + (1 << shift) - 1) >> shift
            }
        }
    }

    pub fn height_for_plane(&self, index: usize) -> usize {
        if index == 0 {
            self.height() as usize
        } else {
            self.chroma_plane_height(index)
        }
    }

    /// Returns the number of bytes per sample for this frame's storage:
    /// 1 for 8-bit formats, 2 for high-bit-depth (10/12/16-bit) formats.
    pub fn bytes_per_sample(&self) -> usize {
        if self.format_raw() & AVM_IMG_FMT_HIGHBITDEPTH != 0 {
            2
        } else {
            1
        }
    }

    /// Returns the number of bytes occupied by one row of active pixel data
    /// in the given plane (i.e. `plane_width(index) * bytes_per_sample`).
    ///
    /// Returns 0 if `index >= 3`.
    pub fn row_bytes(&self, index: usize) -> usize {
        if index >= 3 {
            return 0;
        }
        self.plane_width(index)
            .saturating_mul(self.bytes_per_sample())
    }

    /// Returns a typed view over the full plane storage, choosing
    /// [`PlaneView::U8`] for 8-bit formats and [`PlaneView::U16`] for
    /// high-bit-depth formats.
    ///
    /// Returns `None` under the same conditions as [`Frame::plane`], plus
    /// when the underlying buffer is not properly aligned for `u16` access
    /// (HBD case only — libavm's allocator always satisfies this in
    /// practice, but the check is performed defensively).
    pub fn plane_view(&self, index: usize) -> Option<PlaneView<'_>> {
        let plane = self.plane(index)?;
        if self.bytes_per_sample() == 2 {
            // Verify u16 alignment and even byte length before reinterpreting.
            if plane.as_ptr() as usize % std::mem::align_of::<u16>() != 0 {
                return None;
            }
            if plane.len() % 2 != 0 {
                return None;
            }
            // SAFETY: `plane` is a valid `&[u8]` of the right length and we
            // verified alignment for `u16`.  libavm's allocator always
            // produces correctly aligned plane buffers.  The reinterpreted
            // slice borrows from `Frame<'a>` so the lifetime is correct.
            let u16_slice = unsafe {
                slice::from_raw_parts(plane.as_ptr().cast::<u16>(), plane.len() / 2)
            };
            Some(PlaneView::U16(u16_slice))
        } else {
            Some(PlaneView::U8(plane))
        }
    }

    /// Returns an iterator over the rows of the given plane, each row
    /// cropped to exactly `row_bytes(index)` bytes (i.e. with stride
    /// padding stripped).
    ///
    /// For high-bit-depth content this still yields `&[u8]`; reinterpret
    /// each row to `&[u16]` via `bytemuck::cast_slice` or
    /// [`Frame::plane_view`] if you want typed access.
    ///
    /// Returns `None` under the same conditions as [`Frame::plane`].
    pub fn rows(&self, index: usize) -> Option<impl Iterator<Item = &[u8]> + '_> {
        let plane = self.plane(index)?;
        let stride = self.stride(index);
        let h = self.height_for_plane(index);
        let row_bytes = self.row_bytes(index);
        // `plane` is exactly `stride * h` bytes (validated by `plane()`),
        // so each `chunks(stride)` chunk is exactly `stride` long, and
        // `row_bytes <= stride` is also validated.
        Some(
            plane
                .chunks(stride)
                .take(h)
                .map(move |row| &row[..row_bytes]),
        )
    }

    #[cfg(test)]
    pub(crate) fn from_raw_for_test(img: NonNull<avm_image_t>) -> Frame<'a> {
        Frame {
            img,
            _marker: PhantomData,
        }
    }

    /// Copy this frame's data into an [`OwnedFrame`] that has no lifetime
    /// tie to the [`Decoder`].
    ///
    /// The plane buffers in the returned [`OwnedFrame`] are **packed** —
    /// each row contains exactly `row_bytes(i)` bytes with no per-row
    /// padding — so `strides[i] == row_bytes(i)`.
    ///
    /// `OwnedFrame` is `Send + Sync` and can be moved across thread
    /// boundaries or held across subsequent `Decoder::decode` calls.
    pub fn to_owned(&self) -> OwnedFrame {
        let mut planes = [Vec::new(), Vec::new(), Vec::new()];
        let mut strides = [0usize; 3];
        for (i, plane_buf) in planes.iter_mut().enumerate() {
            let row_bytes = self.row_bytes(i);
            let h = self.height_for_plane(i);
            if let Some(rows) = self.rows(i) {
                plane_buf.reserve_exact(row_bytes.saturating_mul(h));
                for row in rows {
                    plane_buf.extend_from_slice(row);
                }
                strides[i] = row_bytes;
            }
        }
        OwnedFrame {
            width: self.width(),
            height: self.height(),
            bit_depth: self.bit_depth(),
            format: self.format(),
            format_raw: self.format_raw(),
            color_range: self.color_range(),
            chroma_sample_position: self.chroma_sample_position(),
            monochrome: self.monochrome(),
            bytes_per_sample: self.bytes_per_sample(),
            planes,
            strides,
        }
    }
}

/// An owned snapshot of a decoded frame, decoupled from the [`Decoder`]
/// that produced it.
///
/// Plane data is stored **packed** — each row contains exactly
/// `row_bytes` bytes with no stride padding.  This makes `OwnedFrame`
/// `Send + Sync` and safe to hold across subsequent
/// [`Decoder::decode`] calls.
///
/// Construct via [`Frame::to_owned`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedFrame {
    /// Active pixel width.
    pub width: u32,
    /// Active pixel height.
    pub height: u32,
    /// Bit depth of each sample (8, 10, 12, or 16).
    pub bit_depth: u32,
    /// Type-safe pixel format, or `None` if libavm reported an unknown one.
    pub format: Option<PixelFormat>,
    /// Raw libavm `avm_img_fmt` integer for the format.
    pub format_raw: avm_img_fmt,
    /// Sample value range.
    pub color_range: ColorRange,
    /// Chroma sample position.
    pub chroma_sample_position: ChromaSamplePosition,
    /// `true` for monochrome (Y-only) frames.
    pub monochrome: bool,
    /// Bytes per sample (1 for 8-bit, 2 for high-bit-depth).
    pub bytes_per_sample: usize,
    /// Per-plane packed pixel data.  Index 0 is luma, 1/2 are chroma.
    /// Empty for planes that were absent (e.g. chroma on a monochrome
    /// frame) or that failed validation.
    pub planes: [Vec<u8>; 3],
    /// Per-plane row stride in bytes (equals `row_bytes` since the
    /// data is packed).
    pub strides: [usize; 3],
}

impl OwnedFrame {
    /// Returns a slice over the bytes of the given plane, or `None` if
    /// `index >= 3` or the plane is empty.
    pub fn plane(&self, index: usize) -> Option<&[u8]> {
        if index >= 3 {
            return None;
        }
        let p = &self.planes[index];
        if p.is_empty() { None } else { Some(p) }
    }

    /// Returns an iterator over the rows of the given plane.  Each row
    /// is exactly `strides[index]` bytes.
    pub fn rows(&self, index: usize) -> Option<impl Iterator<Item = &[u8]>> {
        let plane = self.plane(index)?;
        let stride = self.strides[index];
        if stride == 0 {
            return None;
        }
        Some(plane.chunks_exact(stride))
    }
}

#[cfg(test)]
mod plane_validation_tests {
    use super::*;
    use std::mem::MaybeUninit;

    fn zeroed_image() -> avm_image_t {
        // SAFETY: avm_image_t is a repr(C) struct of integers/pointers;
        // all-zero is a valid (if degenerate) value.
        unsafe { MaybeUninit::<avm_image_t>::zeroed().assume_init() }
    }

    #[test]
    fn test_plane_rejects_index_out_of_bounds() {
        let mut img = zeroed_image();
        let frame = Frame::from_raw_for_test(NonNull::from(&mut img));
        assert!(frame.plane(3).is_none());
        assert!(frame.plane(4).is_none());
        assert!(frame.plane(usize::MAX).is_none());
    }

    #[test]
    fn test_plane_rejects_null_plane_pointer() {
        let mut img = zeroed_image();
        img.d_w = 320;
        img.d_h = 240;
        img.stride[0] = 320;
        // planes[0] is null from zeroed
        let frame = Frame::from_raw_for_test(NonNull::from(&mut img));
        assert!(frame.plane(0).is_none());
    }

    #[test]
    fn test_plane_rejects_negative_stride() {
        let mut buf = vec![0u8; 1024];
        let mut img = zeroed_image();
        img.d_w = 16;
        img.d_h = 16;
        img.planes[0] = buf.as_mut_ptr();
        img.stride[0] = -1;
        let frame = Frame::from_raw_for_test(NonNull::from(&mut img));
        assert!(frame.plane(0).is_none());
    }

    #[test]
    fn test_plane_rejects_stride_below_row_bytes() {
        let mut buf = vec![0u8; 4096];
        let mut img = zeroed_image();
        img.d_w = 320;
        img.d_h = 240;
        img.planes[0] = buf.as_mut_ptr();
        // stride is 160 but row needs at least 320 bytes (8-bit, no HBD)
        img.stride[0] = 160;
        let frame = Frame::from_raw_for_test(NonNull::from(&mut img));
        assert!(frame.plane(0).is_none());
    }

    // On 64-bit, i32::MAX * u32::MAX < usize::MAX so checked_mul never overflows.
    // This branch is only reachable on 32-bit targets.
    #[test]
    #[cfg(target_pointer_width = "32")]
    fn test_plane_rejects_stride_height_overflow() {
        let mut buf = vec![0u8; 1024];
        let mut img = zeroed_image();
        img.d_w = 1;
        img.d_h = i32::MAX as u32;
        img.planes[0] = buf.as_mut_ptr();
        img.stride[0] = i32::MAX;
        let frame = Frame::from_raw_for_test(NonNull::from(&mut img));
        assert!(frame.plane(0).is_none());
    }

    #[test]
    fn test_plane_rejects_len_exceeds_sz() {
        let mut buf = vec![0u8; 1024 * 1024];
        let mut img = zeroed_image();
        img.d_w = 1024;
        img.d_h = 1024;
        img.planes[0] = buf.as_mut_ptr();
        img.stride[0] = 1024;
        // sz is set to a small value so stride*height > sz
        img.sz = 1024;
        let frame = Frame::from_raw_for_test(NonNull::from(&mut img));
        assert!(frame.plane(0).is_none());
    }

    #[test]
    fn test_plane_returns_valid_slice() {
        let w: usize = 64;
        let h: usize = 48;
        let stride: usize = 128;
        let mut buf = vec![0xABu8; stride * h];
        let mut img = zeroed_image();
        img.d_w = w as u32;
        img.d_h = h as u32;
        img.planes[0] = buf.as_mut_ptr();
        img.stride[0] = stride as i32;
        img.sz = buf.len();
        let frame = Frame::from_raw_for_test(NonNull::from(&mut img));
        let plane = frame.plane(0).expect("plane(0) should return Some");
        assert_eq!(plane.len(), stride * h);
        assert_eq!(plane[0], 0xAB);
    }
}
