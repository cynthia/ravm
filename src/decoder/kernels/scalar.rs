//! Portable scalar kernel implementations.

use super::Kernels;

pub(crate) struct Scalar;

impl Kernels for Scalar {
    fn inv_dct4x4(&self, coeffs: &[i32; 16], dst: &mut [i16], dst_stride: usize) {
        const COS_BIT: i32 = 12;
        const COSPI_16_64: i32 = 11585;
        const COSPI_8_64: i32 = 15137;
        const COSPI_24_64: i32 = 6270;

        let idct4 = |input: [i32; 4]| -> [i32; 4] {
            let a0 = input[0];
            let a1 = input[2];
            let a2 = input[1];
            let a3 = input[3];

            let b0 = (a0 + a1) * COSPI_16_64;
            let b1 = (a0 - a1) * COSPI_16_64;
            let b2 = a2 * COSPI_24_64 - a3 * COSPI_8_64;
            let b3 = a2 * COSPI_8_64 + a3 * COSPI_24_64;

            let rnd = 1 << (COS_BIT - 1);
            let c0 = (b0 + rnd) >> COS_BIT;
            let c1 = (b1 + rnd) >> COS_BIT;
            let c2 = (b2 + rnd) >> COS_BIT;
            let c3 = (b3 + rnd) >> COS_BIT;

            [c0 + c3, c1 + c2, c1 - c2, c0 - c3]
        };

        let mut tmp = [[0i32; 4]; 4];
        for r in 0..4 {
            tmp[r] = idct4([
                coeffs[r * 4],
                coeffs[r * 4 + 1],
                coeffs[r * 4 + 2],
                coeffs[r * 4 + 3],
            ]);
        }

        for c in 0..4 {
            let out = idct4([tmp[0][c], tmp[1][c], tmp[2][c], tmp[3][c]]);
            for r in 0..4 {
                let v = (out[r] + 8) >> 4;
                dst[r * dst_stride + c] = v.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            }
        }
    }
}
