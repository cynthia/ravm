#![forbid(unsafe_code)]
//! Inverse transform dispatch (table-driven outer layer).

use crate::decoder::kernels::Kernels;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TxSize {
    Tx4x4,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum TxType {
    DctDct,
    AdstDct,
    DctAdst,
    AdstAdst,
    FlipadstDct,
    DctFlipadst,
    FlipadstFlipadst,
    AdstFlipadst,
    FlipadstAdst,
    Idtx,
    VDct,
    HDct,
    VAdst,
    HAdst,
    VFlipadst,
    HFlipadst,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum IntraTxFamily {
    DctOnly,
    ExtSet1,
    ExtSet2,
    ShortSide {
        long_side_dct: bool,
        is_rect_horz: bool,
        long_side_64: bool,
    },
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum BaseIntraMode {
    Dc,
    V,
    H,
    D45,
    D135,
    D113,
    D157,
    D203,
    D67,
    Smooth,
    SmoothV,
    SmoothH,
    Paeth,
}

pub(crate) fn base_intra_mode_from_actual_mode(mode: u8) -> Option<BaseIntraMode> {
    match mode {
        0 => Some(BaseIntraMode::Dc),
        1 => Some(BaseIntraMode::V),
        2 => Some(BaseIntraMode::H),
        3 => Some(BaseIntraMode::D45),
        4 => Some(BaseIntraMode::D135),
        5 => Some(BaseIntraMode::D113),
        6 => Some(BaseIntraMode::D157),
        7 => Some(BaseIntraMode::D203),
        8 => Some(BaseIntraMode::D67),
        9 => Some(BaseIntraMode::Smooth),
        10 => Some(BaseIntraMode::SmoothV),
        11 => Some(BaseIntraMode::SmoothH),
        12 => Some(BaseIntraMode::Paeth),
        _ => None,
    }
}

pub(crate) fn default_tx_type_for_base_intra_mode(mode: BaseIntraMode) -> TxType {
    match mode {
        BaseIntraMode::Dc => TxType::DctDct,
        BaseIntraMode::V => TxType::AdstDct,
        BaseIntraMode::H => TxType::DctAdst,
        BaseIntraMode::D45 => TxType::DctDct,
        BaseIntraMode::D135 => TxType::AdstAdst,
        BaseIntraMode::D113 => TxType::AdstDct,
        BaseIntraMode::D157 => TxType::DctAdst,
        BaseIntraMode::D203 => TxType::DctAdst,
        BaseIntraMode::D67 => TxType::AdstDct,
        BaseIntraMode::Smooth => TxType::AdstAdst,
        BaseIntraMode::SmoothV => TxType::AdstDct,
        BaseIntraMode::SmoothH => TxType::DctAdst,
        BaseIntraMode::Paeth => TxType::AdstAdst,
    }
}

#[allow(dead_code)]
pub(crate) fn intra_default_tx_type(intra_mode: u8) -> TxType {
    match intra_mode {
        0 => TxType::DctDct,
        1 => TxType::AdstDct,
        2 => TxType::DctAdst,
        3 => TxType::DctDct,
        4 => TxType::AdstAdst,
        5 => TxType::AdstDct,
        6 => TxType::DctAdst,
        7 => TxType::DctAdst,
        8 => TxType::AdstDct,
        9 => TxType::AdstAdst,
        10 => TxType::AdstDct,
        11 => TxType::DctAdst,
        12 => TxType::AdstAdst,
        _ => TxType::DctDct,
    }
}

#[allow(dead_code)]
pub(crate) fn intra_ext_tx_type_from_symbol(family: IntraTxFamily, symbol: usize) -> TxType {
    match family {
        IntraTxFamily::DctOnly => TxType::DctDct,
        IntraTxFamily::ExtSet1 => match symbol {
            0 => TxType::DctDct,
            1 => TxType::AdstDct,
            2 => TxType::DctAdst,
            3 => TxType::FlipadstDct,
            4 => TxType::DctFlipadst,
            5 => TxType::VDct,
            6 => TxType::HDct,
            _ => TxType::DctDct,
        },
        IntraTxFamily::ExtSet2 => match symbol {
            0 => TxType::DctDct,
            1 => TxType::Idtx,
            _ => TxType::DctDct,
        },
        IntraTxFamily::ShortSide {
            long_side_dct,
            is_rect_horz,
            long_side_64,
        } => match (long_side_64, long_side_dct, is_rect_horz, symbol) {
            (_, true, true, 0) => TxType::DctDct,
            (_, true, true, 1) => TxType::AdstDct,
            (_, true, true, 2) => TxType::FlipadstDct,
            (_, true, true, 3) => TxType::HDct,
            (_, true, false, 0) => TxType::DctDct,
            (_, true, false, 1) => TxType::DctAdst,
            (_, true, false, 2) => TxType::DctFlipadst,
            (_, true, false, 3) => TxType::VDct,
            (false, false, true, 0) => TxType::VDct,
            (false, false, true, 1) => TxType::VAdst,
            (false, false, true, 2) => TxType::VFlipadst,
            (false, false, true, 3) => TxType::Idtx,
            (false, false, false, 0) => TxType::HDct,
            (false, false, false, 1) => TxType::HAdst,
            (false, false, false, 2) => TxType::HFlipadst,
            (false, false, false, 3) => TxType::Idtx,
            (true, false, _, _) => TxType::DctDct,
            _ => TxType::DctDct,
        },
    }
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
        (TxSize::Tx4x4, TxType::AdstDct) => kernels.inv_adstdct4x4(coeffs, dst, stride),
        (TxSize::Tx4x4, TxType::DctAdst) => kernels.inv_dctadst4x4(coeffs, dst, stride),
        (TxSize::Tx4x4, TxType::AdstAdst) => kernels.inv_adstadst4x4(coeffs, dst, stride),
        (TxSize::Tx4x4, TxType::Idtx) => kernels.inv_idtx4x4(coeffs, dst, stride),
        _ => unimplemented!("inverse transform not implemented for {tx_type:?}"),
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

    #[test]
    fn intra_default_tx_type_matches_av2_mapping_subset() {
        assert_eq!(intra_default_tx_type(0), TxType::DctDct);
        assert_eq!(intra_default_tx_type(1), TxType::AdstDct);
        assert_eq!(intra_default_tx_type(2), TxType::DctAdst);
        assert_eq!(intra_default_tx_type(4), TxType::AdstAdst);
    }

    #[test]
    fn base_intra_mode_mapping_matches_actual_predictor_enum() {
        assert_eq!(base_intra_mode_from_actual_mode(0), Some(BaseIntraMode::Dc));
        assert_eq!(base_intra_mode_from_actual_mode(1), Some(BaseIntraMode::V));
        assert_eq!(base_intra_mode_from_actual_mode(2), Some(BaseIntraMode::H));
        assert_eq!(base_intra_mode_from_actual_mode(9), Some(BaseIntraMode::Smooth));
        assert_eq!(base_intra_mode_from_actual_mode(10), Some(BaseIntraMode::SmoothV));
        assert_eq!(base_intra_mode_from_actual_mode(11), Some(BaseIntraMode::SmoothH));
        assert_eq!(base_intra_mode_from_actual_mode(12), Some(BaseIntraMode::Paeth));
        assert_eq!(base_intra_mode_from_actual_mode(13), None);
    }

    #[test]
    fn default_tx_type_for_base_intra_mode_matches_upstream_table() {
        assert_eq!(default_tx_type_for_base_intra_mode(BaseIntraMode::Dc), TxType::DctDct);
        assert_eq!(
            default_tx_type_for_base_intra_mode(BaseIntraMode::Smooth),
            TxType::AdstAdst
        );
        assert_eq!(
            default_tx_type_for_base_intra_mode(BaseIntraMode::SmoothV),
            TxType::AdstDct
        );
        assert_eq!(
            default_tx_type_for_base_intra_mode(BaseIntraMode::SmoothH),
            TxType::DctAdst
        );
        assert_eq!(
            default_tx_type_for_base_intra_mode(BaseIntraMode::Paeth),
            TxType::AdstAdst
        );
    }

    #[test]
    fn short_side_symbol_mapping_matches_av2_layout() {
        assert_eq!(
            intra_ext_tx_type_from_symbol(
                IntraTxFamily::ShortSide {
                    long_side_dct: true,
                    is_rect_horz: true,
                    long_side_64: false,
                },
                1
            ),
            TxType::AdstDct
        );
        assert_eq!(
            intra_ext_tx_type_from_symbol(
                IntraTxFamily::ShortSide {
                    long_side_dct: false,
                    is_rect_horz: false,
                    long_side_64: false,
                },
                3
            ),
            TxType::Idtx
        );
    }

    #[test]
    fn dispatches_to_idtx4x4() {
        let k = detect();
        let mut coeffs = [0i32; 16];
        coeffs[0] = 5;
        coeffs[5] = -3;
        let mut dst = [0i16; 16];
        inverse_transform(k, TxSize::Tx4x4, TxType::Idtx, &coeffs, &mut dst, 4);
        assert_eq!(dst[0], 5);
        assert_eq!(dst[5], -3);
    }

    #[test]
    fn dispatches_to_adstdct4x4() {
        let k = detect();
        let mut coeffs = [0i32; 16];
        coeffs[0] = 32;
        let mut dst = [0i16; 16];
        inverse_transform(k, TxSize::Tx4x4, TxType::AdstDct, &coeffs, &mut dst, 4);
        assert!(dst.iter().any(|&v| v != 0));
    }

    #[test]
    fn dispatches_to_dctadst4x4() {
        let k = detect();
        let mut coeffs = [0i32; 16];
        coeffs[0] = 32;
        let mut dst = [0i16; 16];
        inverse_transform(k, TxSize::Tx4x4, TxType::DctAdst, &coeffs, &mut dst, 4);
        assert!(dst.iter().any(|&v| v != 0));
    }

    #[test]
    fn dispatches_to_adstadst4x4() {
        let k = detect();
        let mut coeffs = [0i32; 16];
        coeffs[0] = 32;
        let mut dst = [0i16; 16];
        inverse_transform(k, TxSize::Tx4x4, TxType::AdstAdst, &coeffs, &mut dst, 4);
        assert!(dst.iter().any(|&v| v != 0));
    }
}
