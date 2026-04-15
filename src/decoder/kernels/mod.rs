//! Kernels trait; runtime dispatch.

pub(crate) mod scalar;

/// Hot-path kernels used by the decoder. SIMD implementations land later.
pub(crate) trait Kernels: Sync + 'static {
    /// Inverse 4x4 DCT_DCT. Input is dequantized coefficients in row-major
    /// order; output is residual samples written into `dst` with stride in
    /// pixels.
    fn inv_dct4x4(&self, coeffs: &[i32; 16], dst: &mut [i16], dst_stride: usize);

    /// Inverse 4x4 IDTX. Input is dequantized coefficients in row-major
    /// order; output is residual samples written into `dst` with stride in
    /// pixels.
    fn inv_idtx4x4(&self, coeffs: &[i32; 16], dst: &mut [i16], dst_stride: usize);

    /// Inverse 4x4 ADST_DCT. ADST in vertical, DCT in horizontal.
    fn inv_adstdct4x4(&self, coeffs: &[i32; 16], dst: &mut [i16], dst_stride: usize);

    /// Inverse 4x4 DCT_ADST. DCT in vertical, ADST in horizontal.
    fn inv_dctadst4x4(&self, coeffs: &[i32; 16], dst: &mut [i16], dst_stride: usize);
}

/// Return the best available kernel implementation for the host CPU.
///
/// M0 returns the scalar implementation unconditionally.
pub(crate) fn detect() -> &'static dyn Kernels {
    &scalar::Scalar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv_dct4x4_all_zeros_produces_all_zeros() {
        let k = detect();
        let coeffs = [0i32; 16];
        let mut dst = [7i16; 16];
        k.inv_dct4x4(&coeffs, &mut dst, 4);
        assert_eq!(dst, [0i16; 16]);
    }

    #[test]
    fn inv_dct4x4_dc_only_produces_flat_block() {
        let k = detect();
        let mut coeffs = [0i32; 16];
        coeffs[0] = 64;
        let mut dst = [0i16; 16];
        k.inv_dct4x4(&coeffs, &mut dst, 4);
        for &v in &dst {
            assert_eq!(v, dst[0], "DC-only block must be flat");
        }
    }

    #[test]
    fn inv_idtx4x4_preserves_all_zero_block() {
        let k = detect();
        let coeffs = [0i32; 16];
        let mut dst = [7i16; 16];
        k.inv_idtx4x4(&coeffs, &mut dst, 4);
        assert_eq!(dst, [0i16; 16]);
    }

    #[test]
    fn inv_adstdct4x4_preserves_all_zero_block() {
        let k = detect();
        let coeffs = [0i32; 16];
        let mut dst = [7i16; 16];
        k.inv_adstdct4x4(&coeffs, &mut dst, 4);
        assert_eq!(dst, [0i16; 16]);
    }

    #[test]
    fn inv_dctadst4x4_preserves_all_zero_block() {
        let k = detect();
        let coeffs = [0i32; 16];
        let mut dst = [7i16; 16];
        k.inv_dctadst4x4(&coeffs, &mut dst, 4);
        assert_eq!(dst, [0i16; 16]);
    }
}
