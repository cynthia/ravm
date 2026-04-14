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
}
