use crate::decoder::{
    DecodeProgress, DecoderError, ErrorKind, FrameBuffer, FrameBufferManager, StreamInfo,
};
use crate::sys::{
    avm_codec_av2_dx, avm_codec_ctx_t, avm_codec_dec_cfg_t, avm_codec_dec_init_ver,
    avm_codec_decode, avm_codec_destroy, avm_codec_frame_buffer_t, avm_codec_get_frame,
    avm_codec_get_stream_info, avm_codec_iter_t, avm_codec_set_frame_buffer_functions,
    avm_codec_stream_info_t, avm_image_t, AVM_DECODER_ABI_VERSION,
};
use std::fmt;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::ptr::NonNull;

pub(crate) struct LibavmDecoder {
    ctx: avm_codec_ctx_t,
    packets_parsed: usize,
    stream_info: Option<StreamInfo>,
    /// Boxed-and-leaked frame-buffer manager handle. Null when no manager
    /// is registered. Reclaimed in `Drop`.
    fb_manager: *mut ManagerHandle,
    // The libavm decoder context has internal mutable state that is unsafe for
    // cross-thread access. Multi-threaded decoding is configured via
    // `avm_codec_dec_cfg_t::threads`. `*const ()` is `!Send + !Sync`.
    _not_send: PhantomData<*const ()>,
}

impl fmt::Debug for LibavmDecoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LibavmDecoder")
            .field("packets_parsed", &self.packets_parsed)
            .field("stream_info", &self.stream_info)
            .field("has_frame_buffer_manager", &!self.fb_manager.is_null())
            .finish_non_exhaustive()
    }
}

struct ManagerHandle {
    inner: Box<dyn FrameBufferManager>,
}

unsafe extern "C" fn fb_get_shim(
    priv_: *mut c_void,
    min_size: usize,
    fb: *mut avm_codec_frame_buffer_t,
) -> c_int {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if priv_.is_null() || fb.is_null() {
            return -1;
        }
        let handle = unsafe { &mut *(priv_ as *mut ManagerHandle) };
        let fb_ref = unsafe { &mut *fb };
        match handle.inner.allocate(min_size) {
            Some(buf) => {
                fb_ref.data = buf.data.as_ptr();
                fb_ref.size = buf.len;
                fb_ref.priv_ = buf.token as *mut c_void;
                0
            }
            None => -1,
        }
    }));
    result.unwrap_or(-1)
}

unsafe extern "C" fn fb_release_shim(
    priv_: *mut c_void,
    fb: *mut avm_codec_frame_buffer_t,
) -> c_int {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if priv_.is_null() || fb.is_null() {
            return -1;
        }
        let handle = unsafe { &mut *(priv_ as *mut ManagerHandle) };
        let fb_ref = unsafe { &*fb };
        let Some(data) = NonNull::new(fb_ref.data) else {
            return -1;
        };
        handle.inner.release(FrameBuffer {
            data,
            len: fb_ref.size,
            token: fb_ref.priv_ as usize,
        });
        0
    }));
    result.unwrap_or(-1)
}

impl LibavmDecoder {
    pub(crate) fn new(threads: Option<u32>) -> Result<Self, DecoderError> {
        unsafe {
            let mut ctx = MaybeUninit::<avm_codec_ctx_t>::uninit();
            let mut cfg = MaybeUninit::<avm_codec_dec_cfg_t>::zeroed();
            if let Some(t) = threads {
                (*cfg.as_mut_ptr()).threads = t;
            }

            let res = avm_codec_dec_init_ver(
                ctx.as_mut_ptr(),
                avm_codec_av2_dx(),
                cfg.as_ptr(),
                0,
                AVM_DECODER_ABI_VERSION as i32,
            );

            match ErrorKind::from_raw(res) {
                None => Ok(Self {
                    ctx: ctx.assume_init(),
                    packets_parsed: 0,
                    stream_info: None,
                    fb_manager: ptr::null_mut(),
                    _not_send: PhantomData,
                }),
                Some(kind) => Err(DecoderError::Init(kind)),
            }
        }
    }

    pub(crate) fn decode(&mut self, data: &[u8]) -> Result<(), DecoderError> {
        unsafe {
            if !data.is_empty() {
                self.packets_parsed += 1;
            }
            let res = avm_codec_decode(&mut self.ctx, data.as_ptr(), data.len(), ptr::null_mut());
            self.refresh_stream_info();
            match ErrorKind::from_raw(res) {
                None => Ok(()),
                Some(kind) => Err(DecoderError::Decode(kind)),
            }
        }
    }

    pub(crate) fn flush(&mut self) -> Result<(), DecoderError> {
        unsafe {
            let res = avm_codec_decode(&mut self.ctx, ptr::null(), 0, ptr::null_mut());
            match ErrorKind::from_raw(res) {
                None => {
                    self.refresh_stream_info();
                    Ok(())
                }
                Some(kind) => Err(DecoderError::Flush(kind)),
            }
        }
    }

    pub(crate) fn get_stream_info(&mut self) -> Result<StreamInfo, DecoderError> {
        unsafe {
            let mut si = MaybeUninit::<avm_codec_stream_info_t>::zeroed();
            let res = avm_codec_get_stream_info(&mut self.ctx, si.as_mut_ptr());
            match ErrorKind::from_raw(res) {
                None => {
                    let si = si.assume_init();
                    let info = StreamInfo {
                        width: si.w,
                        height: si.h,
                        is_kf: si.is_kf != 0,
                        number_tlayers: si.number_tlayers,
                        number_mlayers: si.number_mlayers,
                        number_xlayers: si.number_xlayers,
                    };
                    if info.width != 0 && info.height != 0 {
                        self.stream_info = Some(info);
                    }
                    Ok(info)
                }
                Some(kind) => Err(DecoderError::StreamInfo(kind)),
            }
        }
    }

    pub(crate) unsafe fn set_frame_buffer_functions(
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
        let res = unsafe {
            avm_codec_set_frame_buffer_functions(
                &mut self.ctx,
                Some(get_fb),
                Some(release_fb),
                priv_,
            )
        };

        match ErrorKind::from_raw(res) {
            None => Ok(()),
            Some(kind) => Err(DecoderError::SetFrameBufferFunctions(kind)),
        }
    }

    pub(crate) fn set_frame_buffer_manager<M: FrameBufferManager + 'static>(
        &mut self,
        manager: M,
    ) -> Result<(), DecoderError> {
        let handle = Box::new(ManagerHandle {
            inner: Box::new(manager),
        });
        let handle_ptr = Box::into_raw(handle);

        let result = unsafe {
            self.set_frame_buffer_functions(fb_get_shim, fb_release_shim, handle_ptr.cast())
        };

        match result {
            Ok(()) => {
                if !self.fb_manager.is_null() {
                    unsafe { drop(Box::from_raw(self.fb_manager)) };
                }
                self.fb_manager = handle_ptr;
                Ok(())
            }
            Err(e) => {
                unsafe { drop(Box::from_raw(handle_ptr)) };
                Err(e)
            }
        }
    }

    pub(crate) fn get_frame(&mut self, iter: &mut avm_codec_iter_t) -> Option<NonNull<avm_image_t>> {
        unsafe { NonNull::new(avm_codec_get_frame(&mut self.ctx, iter) as *mut avm_image_t) }
    }

    pub(crate) fn progress(&self) -> DecodeProgress {
        DecodeProgress {
            backend: crate::backend::BackendKind::Libavm,
            packets_parsed: Some(self.packets_parsed),
            obus_parsed: None,
            frame_packets_seen: None,
            sequence_header: None,
            stream_info: self.stream_info,
            last_frame_packet_kind: None,
            last_frame_header: None,
            last_event: None,
            recent_events: [None; 4],
        }
    }

    fn refresh_stream_info(&mut self) {
        unsafe {
            let mut si = MaybeUninit::<avm_codec_stream_info_t>::zeroed();
            let res = avm_codec_get_stream_info(&mut self.ctx, si.as_mut_ptr());
            if ErrorKind::from_raw(res).is_none() {
                let si = si.assume_init();
                let info = StreamInfo {
                    width: si.w,
                    height: si.h,
                    is_kf: si.is_kf != 0,
                    number_tlayers: si.number_tlayers,
                    number_mlayers: si.number_mlayers,
                    number_xlayers: si.number_xlayers,
                };
                if info.width != 0 && info.height != 0 {
                    self.stream_info = Some(info);
                }
            }
        }
    }
}

impl Drop for LibavmDecoder {
    fn drop(&mut self) {
        unsafe {
            avm_codec_destroy(&mut self.ctx);
        }
        if !self.fb_manager.is_null() {
            unsafe { drop(Box::from_raw(self.fb_manager)) };
        }
    }
}
