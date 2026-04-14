#![forbid(unsafe_code)]
//! Inverse transform dispatch (table-driven outer layer).

use crate::decoder::kernels::Kernels;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TxSize {
    Tx4x4,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TxType {
    DctDct,
}

pub(crate) fn inverse_transform(
    kernels: &dyn Kernels,
    tx_size: TxSize,
    tx_type: TxType,
    coeffs: &[i32; 16],
    dst: &mut [i16],
    stride: usize,
) {
    match (tx_size, tx_type) {
        (TxSize::Tx4x4, TxType::DctDct) => kernels.inv_dct4x4(coeffs, dst, stride),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::kernels::detect;

    #[test]
    fn dispatches_to_dct4x4() {
        let k = detect();
        let coeffs = [0i32; 16];
        let mut dst = [1i16; 16];
        inverse_transform(k, TxSize::Tx4x4, TxType::DctDct, &coeffs, &mut dst, 4);
        assert_eq!(dst, [0i16; 16]);
    }
}
