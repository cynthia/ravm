//! Safe Rust bindings for the AVM (AV2) video decoder.
//!
//! See [`decoder::Decoder`] for the main entry point and [`ivf::IvfReader`]
//! for parsing IVF container files.  Type-safe enums for pixel formats and
//! colorspaces live in [`format`]; raw bindgen FFI types are tucked away in
//! [`ffi`] for advanced or test use.

/// Raw bindgen-generated FFI bindings to libavm.
///
/// This is the firehose: every symbol from `avm/avm_decoder.h` and
/// `avm/avmdx.h` is here under its bindgen-generated name.  Most users
/// should prefer the curated re-exports in [`ffi`] or, better, the safe
/// wrappers in [`decoder`], [`format`], and [`ivf`].
#[allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code,
    unused_imports
)]
pub mod sys {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub mod decoder;
pub mod format;
pub mod ivf;
pub mod stream;

pub use format::{ChromaSamplePosition, ColorRange, PixelFormat, Subsampling};
pub use stream::{decode_ivf, decode_ivf_reader, StreamError};

/// Raw FFI bindings to libavm.
///
/// Most users should not need this module — the safe wrappers in
/// [`decoder`] and the type-safe enums in [`format`] are the intended
/// public API.  These re-exports exist to support tests, advanced users,
/// and use cases not yet covered by the safe API.
pub mod ffi {
    pub use crate::sys::{
        avm_codec_av2_dx, avm_codec_ctx_t, avm_codec_dec_cfg_t, avm_codec_dec_init_ver,
        avm_codec_decode, avm_codec_destroy, avm_codec_err_t,
        avm_codec_err_t_AVM_CODEC_INVALID_PARAM, avm_codec_err_t_AVM_CODEC_MEM_ERROR,
        avm_codec_err_t_AVM_CODEC_OK, avm_codec_frame_buffer_t, avm_codec_get_frame,
        avm_codec_get_stream_info, avm_codec_iter_t, avm_codec_set_frame_buffer_functions,
        avm_codec_stream_info_t, avm_codec_version_str, avm_image_t, AVM_DECODER_ABI_VERSION,
        AVM_MAXIMUM_REF_BUFFERS, AVM_MAXIMUM_WORK_BUFFERS,
    };
}

#[cfg(test)]
mod tests {
    use crate::ffi::*;
    use std::ffi::CStr;

    #[test]
    fn test_codec_version() {
        // SAFETY: avm_codec_version_str returns a pointer to a 'static C string.
        let version = unsafe { CStr::from_ptr(avm_codec_version_str()) };
        println!("AVM Codec Version: {version:?}");
        assert!(!version.to_bytes().is_empty());
    }

    #[test]
    fn test_decoder_init() {
        // SAFETY: We construct ctx via MaybeUninit and only assume_init on success.
        // The C library fully initializes ctx when avm_codec_dec_init_ver returns OK.
        unsafe {
            let mut ctx = std::mem::MaybeUninit::<avm_codec_ctx_t>::uninit();
            let iface = avm_codec_av2_dx();
            let res = avm_codec_dec_init_ver(
                ctx.as_mut_ptr(),
                iface,
                std::ptr::null(),
                0,
                AVM_DECODER_ABI_VERSION as i32,
            );
            assert_eq!(res, avm_codec_err_t_AVM_CODEC_OK);
            let mut ctx = ctx.assume_init();
            avm_codec_destroy(&mut ctx);
        }
    }

    #[test]
    fn test_safe_decoder() {
        let decoder = crate::decoder::Decoder::new();
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_decoder_with_threads() {
        let decoder = crate::decoder::Decoder::builder().threads(4).build();
        assert!(decoder.is_ok());
    }
}
