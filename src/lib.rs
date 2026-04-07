#[allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code
)]
mod sys {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
pub use sys::*;

pub mod decoder;
pub mod ivf;

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CStr;

    #[test]
    fn test_codec_version() {
        let version = unsafe { CStr::from_ptr(avm_codec_version_str()) };
        println!("AVM Codec Version: {version:?}");
        assert!(!version.to_bytes().is_empty());
    }

    #[test]
    fn test_decoder_init() {
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
        let decoder = decoder::Decoder::new();
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_decoder_with_threads() {
        let decoder = decoder::Decoder::with_config(Some(4));
        assert!(decoder.is_ok());
    }
}
