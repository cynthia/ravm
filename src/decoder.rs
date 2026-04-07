use crate::{
    avm_codec_av2_dx, avm_codec_ctx_t, avm_codec_dec_cfg_t, avm_codec_dec_init_ver,
    avm_codec_decode, avm_codec_destroy, avm_codec_err_t, avm_codec_err_t_AVM_CODEC_OK,
    avm_codec_frame_buffer_t, avm_codec_get_frame, avm_codec_get_stream_info, avm_codec_iter_t,
    avm_codec_set_frame_buffer_functions, avm_codec_stream_info_t, avm_image_t, avm_img_fmt,
    AVM_DECODER_ABI_VERSION,
};
use std::fmt;
use std::marker::PhantomData;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::ptr::NonNull;
use std::slice;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderError {
    CodecError(avm_codec_err_t),
    InitFailed(avm_codec_err_t),
    DecodeFailed(avm_codec_err_t),
    Incapable,
}

impl fmt::Display for DecoderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecoderError::CodecError(e) => write!(f, "Codec error: {e}"),
            DecoderError::InitFailed(e) => write!(f, "Initialization failed: {e}"),
            DecoderError::DecodeFailed(e) => write!(f, "Decode failed: {e}"),
            DecoderError::Incapable => write!(f, "Decoder is incapable of this operation"),
        }
    }
}

impl std::error::Error for DecoderError {}

pub struct Decoder {
    ctx: avm_codec_ctx_t,
    // The libavm decoder context has internal mutable state that is unsafe for cross-thread
    // access.  Multi-threaded decoding is configured via `avm_codec_dec_cfg_t::threads`.
    // `*const ()` is `!Send + !Sync`, making this explicit.
    _not_send: PhantomData<*const ()>,
}

impl Decoder {
    pub fn new() -> Result<Self, DecoderError> {
        Self::with_config(None)
    }

    pub fn with_config(threads: Option<u32>) -> Result<Self, DecoderError> {
        use std::mem::MaybeUninit;
        // SAFETY: `avm_codec_av2_dx()` returns a non-null static interface pointer.
        // `cfg` is zeroed, which is valid for `avm_codec_dec_cfg_t` (all-integer fields).
        // `avm_codec_dec_init_ver` fully initializes `ctx` when it returns AVM_CODEC_OK;
        // we only call `assume_init()` on that success path.
        unsafe {
            let mut ctx = MaybeUninit::<avm_codec_ctx_t>::uninit();
            let mut cfg = MaybeUninit::<avm_codec_dec_cfg_t>::zeroed();
            if let Some(t) = threads {
                // SAFETY: zeroed() produces a valid avm_codec_dec_cfg_t (all-integer fields).
                (*cfg.as_mut_ptr()).threads = t;
            }

            let iface = avm_codec_av2_dx();
            let res = avm_codec_dec_init_ver(
                ctx.as_mut_ptr(),
                iface,
                cfg.as_ptr(),
                0,
                AVM_DECODER_ABI_VERSION as i32,
            );

            if res == avm_codec_err_t_AVM_CODEC_OK {
                // SAFETY: avm_codec_dec_init_ver fully initializes ctx on success.
                Ok(Self {
                    ctx: ctx.assume_init(),
                    _not_send: PhantomData,
                })
            } else {
                Err(DecoderError::InitFailed(res))
            }
        }
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
        // SAFETY: `self.ctx` was fully initialized in `with_config()`.
        // `data.as_ptr()` is valid for `data.len()` bytes (guaranteed by slice invariants).
        // `avm_codec_decode` processes data synchronously and does not retain the pointer.
        unsafe {
            let res = avm_codec_decode(
                &mut self.ctx,
                data.as_ptr(),
                data.len(),
                ptr::null_mut(),
            );

            if res == avm_codec_err_t_AVM_CODEC_OK {
                Ok(())
            } else {
                Err(DecoderError::DecodeFailed(res))
            }
        }
    }

    /// Flush the decoder, signaling end-of-stream.
    ///
    /// After the last packet has been decoded, call `flush()` followed by
    /// `get_frames()` to retrieve any buffered frames (e.g. due to B-frame
    /// reordering). Equivalent to the C idiom `avm_codec_decode(ctx, NULL, 0, NULL)`.
    pub fn flush(&mut self) -> Result<(), DecoderError> {
        // SAFETY: Passing null data with size 0 signals EOF to the codec.
        // The codec does not dereference the null pointer.
        unsafe {
            let res = avm_codec_decode(
                &mut self.ctx,
                ptr::null(),
                0,
                ptr::null_mut(),
            );
            if res == avm_codec_err_t_AVM_CODEC_OK {
                Ok(())
            } else {
                Err(DecoderError::DecodeFailed(res))
            }
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
            iter: ptr::null(),
        }
    }

    pub fn get_stream_info(&mut self) -> Result<StreamInfo, DecoderError> {
        use std::mem::MaybeUninit;
        // SAFETY: `self.ctx` was fully initialized in `with_config()`. `si` is zeroed
        // (valid for all-integer `avm_codec_stream_info_t`). `avm_codec_get_stream_info`
        // fully initializes `si` on AVM_CODEC_OK; we only call `assume_init()` on success.
        unsafe {
            let mut si = MaybeUninit::<avm_codec_stream_info_t>::zeroed();
            let res = avm_codec_get_stream_info(&mut self.ctx, si.as_mut_ptr());
            if res == avm_codec_err_t_AVM_CODEC_OK {
                // SAFETY: avm_codec_get_stream_info fully initializes si on success.
                let si = si.assume_init();
                Ok(StreamInfo {
                    width: si.w,
                    height: si.h,
                    is_kf: si.is_kf != 0,
                    number_tlayers: si.number_tlayers,
                    number_mlayers: si.number_mlayers,
                    number_xlayers: si.number_xlayers,
                })
            } else {
                Err(DecoderError::CodecError(res))
            }
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
        // SAFETY: `self.ctx` was fully initialized in `with_config()`. The caller
        // guarantees (via the outer `unsafe fn` contract) that `priv_` outlives the
        // `Decoder` and that callbacks do not panic (unwinding through C is UB).
        unsafe {
            let res = avm_codec_set_frame_buffer_functions(
                &mut self.ctx,
                Some(get_fb),
                Some(release_fb),
                priv_,
            );

            if res == avm_codec_err_t_AVM_CODEC_OK {
                Ok(())
            } else {
                Err(DecoderError::CodecError(res))
            }
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
        unsafe {
            let img_ptr = avm_codec_get_frame(&mut self.decoder.ctx, &mut self.iter);
            NonNull::new(img_ptr as *mut avm_image_t).map(|img| Frame {
                img,
                _marker: PhantomData,
            })
        }
    }
}

impl Drop for Decoder {
    // Safety note: a panic inside a registered frame-buffer callback cannot unwind through
    // the `extern "C"` boundary — since Rust 1.71 the runtime aborts the process instead of
    // exhibiting UB.  `avm_codec_destroy` therefore never runs during such an unwind.  If a
    // future safe callback wrapper wants panics to be recoverable, it must wrap the closure
    // body in `std::panic::catch_unwind` and translate the panic into an error code.
    fn drop(&mut self) {
        // SAFETY: `self.ctx` was fully initialized in `with_config()`.
        // `avm_codec_destroy` is called exactly once — Rust's ownership model
        // guarantees a single `drop()` call.
        unsafe {
            avm_codec_destroy(&mut self.ctx);
        }
    }
}

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

impl<'a> Frame<'a> {
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

    pub fn format(&self) -> avm_img_fmt {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        unsafe { (*self.img.as_ptr()).fmt }
    }

    pub fn monochrome(&self) -> bool {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        unsafe { (*self.img.as_ptr()).monochrome != 0 }
    }

    pub fn chroma_sample_position(&self) -> crate::avm_chroma_sample_position_t {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        unsafe { (*self.img.as_ptr()).csp }
    }

    pub fn color_range(&self) -> crate::avm_color_range_t {
        // SAFETY: `self.img` is non-null (NonNull invariant). The pointee is valid
        // for the lifetime `'a` which borrows the Decoder, preventing use-after-free.
        unsafe { (*self.img.as_ptr()).range }
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
            let bps: usize = if ((*img).fmt & crate::AVM_IMG_FMT_HIGHBITDEPTH) != 0 { 2 } else { 1 };
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

    #[cfg(test)]
    pub(crate) fn from_raw_for_test(img: NonNull<avm_image_t>) -> Frame<'a> {
        Frame {
            img,
            _marker: PhantomData,
        }
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
