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
#[allow(dead_code)]
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
#[allow(dead_code)]
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

/// Directional 45-degree intra prediction for a 4x4 block.
pub(crate) fn predict_d45_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 9]>,
    dst: &mut [P],
    stride: usize,
) {
    predict_z1_4x4(above, 64, dst, stride);
}

/// Directional 67-degree intra prediction for a 4x4 block.
pub(crate) fn predict_d67_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 9]>,
    dst: &mut [P],
    stride: usize,
) {
    predict_z1_4x4(above, 24, dst, stride);
}

/// Directional 203-degree intra prediction for a 4x4 block.
pub(crate) fn predict_d203_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    left: Option<&[P; 9]>,
    dst: &mut [P],
    stride: usize,
) {
    predict_z3_4x4(left, 227, dst, stride);
}

/// Directional 157-degree intra prediction for a 4x4 block.
pub(crate) fn predict_d157_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    left: Option<&[P; 4]>,
    above_left: Option<P>,
    dst: &mut [P],
    stride: usize,
) {
    predict_z2_4x4(above, left, above_left, 170, 56, dst, stride);
}

/// Directional 113-degree intra prediction for a 4x4 block.
pub(crate) fn predict_d113_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    left: Option<&[P; 4]>,
    above_left: Option<P>,
    dst: &mut [P],
    stride: usize,
) {
    predict_z2_4x4(above, left, above_left, 24, 178, dst, stride);
}

fn predict_z1_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 9]>,
    dx: i32,
    dst: &mut [P],
    stride: usize,
) {
    let fill = [mid_gray(); 8];
    let mut extended = [mid_gray(); 9];
    if let Some(above) = above {
        extended = *above;
    } else {
        extended[..8].copy_from_slice(&fill);
        extended[8] = fill[7];
    }

    for y in 0..4 {
        let x = dx * (y as i32 + 1);
        let mut base = (x >> 6) as usize;
        let shift = ((x & 0x3f) >> 1) as u32;
        for x in 0..4 {
            let a: u32 = extended[base].into();
            let b: u32 = extended[(base + 1).min(8)].into();
            let val = a * (32 - shift) + b * shift;
            dst[y * stride + x] = P::try_from(divide_round(val, 5)).ok().unwrap_or_default();
            base += 1;
        }
    }
}

fn predict_z3_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    left: Option<&[P; 9]>,
    dy: i32,
    dst: &mut [P],
    stride: usize,
) {
    let fill = [mid_gray(); 8];
    let mut extended = [mid_gray(); 9];
    if let Some(left) = left {
        extended = *left;
    } else {
        extended[..8].copy_from_slice(&fill);
        extended[8] = fill[7];
    }

    for x in 0..4 {
        let y = dy * (x as i32 + 1);
        let mut base = (y >> 6) as usize;
        let shift = ((y & 0x3f) >> 1) as u32;
        for row in 0..4 {
            let sample = if base < 7 {
                let a: u32 = extended[base].into();
                let b: u32 = extended[(base + 1).min(8)].into();
                P::try_from(divide_round(a * (32 - shift) + b * shift, 5))
                    .ok()
                    .unwrap_or_default()
            } else {
                extended[7]
            };
            dst[row * stride + x] = sample;
            base += 1;
        }
    }
}

fn predict_z2_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    left: Option<&[P; 4]>,
    above_left: Option<P>,
    dx: i32,
    dy: i32,
    dst: &mut [P],
    stride: usize,
) {
    let fill = [mid_gray(); 4];
    let above = above.copied().unwrap_or(fill);
    let left = left.copied().unwrap_or(fill);
    let above_left = above_left.unwrap_or_else(mid_gray);
    let above_ext = [above_left, above[0], above[1], above[2], above[3], above[3]];
    let left_ext = [above_left, left[0], left[1], left[2], left[3], left[3]];

    for row in 0..4 {
        for col in 0..4 {
            let y = row as i32 + 1;
            let x = ((col as i32) << 6) - y * dx;
            let base_x = x >> 6;
            let val = if base_x >= -1 {
                let shift = ((x & 0x3f) >> 1) as u32;
                let a: u32 = above_ext[(base_x + 1) as usize].into();
                let b: u32 = above_ext[(base_x + 2) as usize].into();
                divide_round(a * (32 - shift) + b * shift, 5)
            } else {
                let x = col as i32 + 1;
                let y = ((row as i32) << 6) - x * dy;
                let base_y = y >> 6;
                let shift = ((y & 0x3f) >> 1) as u32;
                let a: u32 = left_ext[(base_y + 1).max(0) as usize].into();
                let b: u32 = left_ext[(base_y + 2).max(0) as usize].into();
                divide_round(a * (32 - shift) + b * shift, 5)
            };
            dst[row * stride + col] = P::try_from(val).ok().unwrap_or_default();
        }
    }
}

fn mid_gray<P: Pixel + Into<u32> + TryFrom<u32>>() -> P {
    P::try_from(1u32 << (P::BIT_DEPTH - 1)).ok().unwrap_or_default()
}

fn divide_round(value: u32, bits: u32) -> u32 {
    (value + (1 << (bits - 1))) >> bits
}

fn abs_diff(a: i32, b: i32) -> i32 {
    if a > b { a - b } else { b - a }
}

fn paeth_predictor_single(left: u32, top: u32, top_left: u32) -> u32 {
    let left = left as i32;
    let top = top as i32;
    let top_left = top_left as i32;
    let base = top + left - top_left;
    let p_left = abs_diff(base, left);
    let p_top = abs_diff(base, top);
    let p_top_left = abs_diff(base, top_left);
    if p_left <= p_top && p_left <= p_top_left {
        left as u32
    } else if p_top <= p_top_left {
        top as u32
    } else {
        top_left as u32
    }
}

/// Smooth intra prediction for a 4x4 block.
pub(crate) fn predict_smooth_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    left: Option<&[P; 4]>,
    dst: &mut [P],
    stride: usize,
) {
    let fill = [mid_gray(); 4];
    let above = above.copied().unwrap_or(fill);
    let left = left.copied().unwrap_or(fill);
    let below_pred: u32 = left[3].into();
    let right_pred: u32 = above[3].into();
    const WEIGHTS: [u32; 4] = [255, 149, 85, 64];
    const SCALE: u32 = 1 << 8;
    const LOG2_SCALE: u32 = 9;

    for y in 0..4 {
        for x in 0..4 {
            let this_pred = WEIGHTS[y] * u32::from(Into::<u32>::into(above[x]) as u16)
                + (SCALE - WEIGHTS[y]) * below_pred
                + WEIGHTS[x] * u32::from(Into::<u32>::into(left[y]) as u16)
                + (SCALE - WEIGHTS[x]) * right_pred;
            dst[y * stride + x] = P::try_from(divide_round(this_pred, LOG2_SCALE))
                .ok()
                .unwrap_or_default();
        }
    }
}

/// Vertical smooth intra prediction for a 4x4 block.
pub(crate) fn predict_smooth_v_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    left: Option<&[P; 4]>,
    dst: &mut [P],
    stride: usize,
) {
    let fill = [mid_gray(); 4];
    let above = above.copied().unwrap_or(fill);
    let left = left.copied().unwrap_or(fill);
    let below_pred: u32 = left[3].into();
    const WEIGHTS: [u32; 4] = [255, 149, 85, 64];
    const SCALE: u32 = 1 << 8;

    for y in 0..4 {
        for x in 0..4 {
            let top: u32 = above[x].into();
            let this_pred = WEIGHTS[y] * top + (SCALE - WEIGHTS[y]) * below_pred;
            dst[y * stride + x] = P::try_from(divide_round(this_pred, 8))
                .ok()
                .unwrap_or_default();
        }
    }
}

/// Horizontal smooth intra prediction for a 4x4 block.
pub(crate) fn predict_smooth_h_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    left: Option<&[P; 4]>,
    dst: &mut [P],
    stride: usize,
) {
    let fill = [mid_gray(); 4];
    let above = above.copied().unwrap_or(fill);
    let left = left.copied().unwrap_or(fill);
    let right_pred: u32 = above[3].into();
    const WEIGHTS: [u32; 4] = [255, 149, 85, 64];
    const SCALE: u32 = 1 << 8;

    for y in 0..4 {
        let left_sample: u32 = left[y].into();
        for x in 0..4 {
            let this_pred = WEIGHTS[x] * left_sample + (SCALE - WEIGHTS[x]) * right_pred;
            dst[y * stride + x] = P::try_from(divide_round(this_pred, 8))
                .ok()
                .unwrap_or_default();
        }
    }
}

/// Paeth intra prediction for a 4x4 block.
pub(crate) fn predict_paeth_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    left: Option<&[P; 4]>,
    above_left: Option<P>,
    dst: &mut [P],
    stride: usize,
) {
    let fill = [mid_gray(); 4];
    let above = above.copied().unwrap_or(fill);
    let left = left.copied().unwrap_or(fill);
    let above_left: u32 = above_left.unwrap_or_else(mid_gray).into();

    for y in 0..4 {
        let left_sample: u32 = left[y].into();
        for x in 0..4 {
            let top_sample: u32 = above[x].into();
            let pred = paeth_predictor_single(left_sample, top_sample, above_left);
            dst[y * stride + x] = P::try_from(pred).ok().unwrap_or_default();
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

    #[test]
    fn smooth_uses_both_edges() {
        let above = [10u8, 20, 30, 40];
        let left = [50u8, 60, 70, 80];
        let mut dst = [0u8; 16];
        predict_smooth_4x4::<u8>(Some(&above), Some(&left), &mut dst, 4);
        assert_eq!(dst, [30, 33, 37, 41, 50, 48, 49, 51, 63, 59, 57, 57, 71, 64, 60, 60]);
    }

    #[test]
    fn smooth_v_blends_top_toward_bottom_left() {
        let above = [10u8, 20, 30, 40];
        let left = [50u8, 60, 70, 80];
        let mut dst = [0u8; 16];
        predict_smooth_v_4x4::<u8>(Some(&above), Some(&left), &mut dst, 4);
        assert_eq!(dst, [10, 20, 30, 40, 39, 45, 51, 57, 57, 60, 63, 67, 63, 65, 68, 70]);
    }

    #[test]
    fn smooth_h_blends_left_toward_top_right() {
        let above = [10u8, 20, 30, 40];
        let left = [50u8, 60, 70, 80];
        let mut dst = [0u8; 16];
        predict_smooth_h_4x4::<u8>(Some(&above), Some(&left), &mut dst, 4);
        assert_eq!(dst, [50, 46, 43, 43, 60, 52, 47, 45, 70, 57, 50, 48, 80, 63, 53, 50]);
    }

    #[test]
    fn paeth_selects_nearest_reference() {
        let above = [10u8, 20, 30, 40];
        let left = [50u8, 60, 70, 80];
        let mut dst = [0u8; 16];
        predict_paeth_4x4::<u8>(Some(&above), Some(&left), Some(15), &mut dst, 4);
        assert_eq!(dst, [50, 50, 50, 50, 60, 60, 60, 60, 70, 70, 70, 70, 80, 80, 80, 80]);
    }

    #[test]
    fn d45_walks_down_the_top_reference() {
        let above = [1u8, 2, 3, 4, 5, 6, 7, 8, 8];
        let mut dst = [0u8; 16];
        predict_d45_4x4::<u8>(Some(&above), &mut dst, 4);
        assert_eq!(dst, [2, 3, 4, 5, 3, 4, 5, 6, 4, 5, 6, 7, 5, 6, 7, 8]);
    }

    #[test]
    fn d67_uses_fractional_top_interpolation() {
        let above = [1u8, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut dst = [0u8; 16];
        predict_d67_4x4::<u8>(Some(&above), &mut dst, 4);
        assert_eq!(dst, [1, 2, 3, 4, 2, 3, 4, 5, 2, 3, 4, 5, 3, 4, 5, 6]);
    }

    #[test]
    fn d203_uses_fractional_left_interpolation() {
        let left = [1u8, 2, 3, 4, 5, 6, 7, 8, 9];
        let mut dst = [0u8; 16];
        predict_d203_4x4::<u8>(Some(&left), &mut dst, 4);
        assert_eq!(dst, [5, 8, 8, 8, 6, 8, 8, 8, 7, 8, 8, 8, 8, 8, 8, 8]);
    }

    #[test]
    fn d157_blends_top_and_left_references() {
        let above = [10u8, 20, 30, 40];
        let left = [50u8, 60, 70, 80];
        let mut dst = [0u8; 16];
        predict_d157_4x4::<u8>(Some(&above), Some(&left), Some(15), &mut dst, 4);
        assert_eq!(dst, [19, 15, 13, 13, 51, 24, 15, 15, 61, 53, 28, 15, 71, 63, 54, 33]);
    }

    #[test]
    fn d113_blends_top_and_left_references() {
        let above = [10u8, 20, 30, 40];
        let left = [50u8, 60, 70, 80];
        let mut dst = [0u8; 16];
        predict_d113_4x4::<u8>(Some(&above), Some(&left), Some(15), &mut dst, 4);
        assert_eq!(dst, [12, 16, 26, 36, 14, 13, 23, 33, 23, 11, 19, 29, 52, 13, 15, 25]);
    }
}
