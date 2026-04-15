#![forbid(unsafe_code)]
//! Intra prediction.

use crate::decoder::frame_buffer::Pixel;

/// DC intra prediction for a 4x4 block.
pub(crate) fn predict_dc_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    left: Option<&[P; 4]>,
    dst: &mut [P],
    stride: usize,
) {
    let dc = match (above, left) {
        (Some(above), Some(left)) => {
            let sum_above: u32 = above.iter().copied().map(Into::into).sum();
            let sum_left: u32 = left.iter().copied().map(Into::into).sum();
            (sum_above + sum_left + 4) >> 3
        }
        (Some(above), None) => {
            let sum_above: u32 = above.iter().copied().map(Into::into).sum();
            (sum_above + 2) >> 2
        }
        (None, Some(left)) => {
            let sum_left: u32 = left.iter().copied().map(Into::into).sum();
            (sum_left + 2) >> 2
        }
        (None, None) => 1u32 << (P::BIT_DEPTH - 1),
    };

    let dc = P::try_from(dc).ok().unwrap_or_default();
    for y in 0..4 {
        for x in 0..4 {
            dst[y * stride + x] = dc;
        }
    }
}

/// Vertical intra prediction for a 4x4 block.
pub(crate) fn predict_v_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    dst: &mut [P],
    stride: usize,
) {
    let fill = above
        .copied()
        .unwrap_or([P::try_from(1u32 << (P::BIT_DEPTH - 1)).ok().unwrap_or_default(); 4]);
    for y in 0..4 {
        for x in 0..4 {
            dst[y * stride + x] = fill[x];
        }
    }
}

/// Horizontal intra prediction for a 4x4 block.
pub(crate) fn predict_h_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    left: Option<&[P; 4]>,
    dst: &mut [P],
    stride: usize,
) {
    let fill = left
        .copied()
        .unwrap_or([P::try_from(1u32 << (P::BIT_DEPTH - 1)).ok().unwrap_or_default(); 4]);
    for y in 0..4 {
        for x in 0..4 {
            dst[y * stride + x] = fill[y];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_no_neighbors_produces_mid_gray() {
        let mut dst = [0u8; 16];
        predict_dc_4x4::<u8>(None, None, &mut dst, 4);
        assert_eq!(dst, [128u8; 16]);
    }

    #[test]
    fn dc_with_neighbors_averages_both_sides() {
        let above = [100u8; 4];
        let left = [200u8; 4];
        let mut dst = [0u8; 16];
        predict_dc_4x4::<u8>(Some(&above), Some(&left), &mut dst, 4);
        assert_eq!(dst, [150u8; 16]);
    }

    #[test]
    fn v_uses_above_samples_per_column() {
        let above = [1u8, 2, 3, 4];
        let mut dst = [0u8; 16];
        predict_v_4x4::<u8>(Some(&above), &mut dst, 4);
        assert_eq!(dst, [1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4, 1, 2, 3, 4]);
    }

    #[test]
    fn h_uses_left_samples_per_row() {
        let left = [5u8, 6, 7, 8];
        let mut dst = [0u8; 16];
        predict_h_4x4::<u8>(Some(&left), &mut dst, 4);
        assert_eq!(dst, [5, 5, 5, 5, 6, 6, 6, 6, 7, 7, 7, 7, 8, 8, 8, 8]);
    }
}
