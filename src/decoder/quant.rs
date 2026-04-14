#![forbid(unsafe_code)]
//! Dequantization and quantization matrices.

use crate::bitstream::UncompressedFrameHeader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plane {
    Y,
    U,
    V,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct QuantContext {
    pub base_q_idx: u8,
    pub delta_q_y_dc: i8,
    pub delta_q_u_dc: i8,
    pub delta_q_u_ac: i8,
    pub delta_q_v_dc: i8,
    pub delta_q_v_ac: i8,
    pub using_qmatrix: bool,
    pub qm_y: u8,
    pub qm_u: u8,
    pub qm_v: u8,
}

impl QuantContext {
    pub fn from_frame_header(header: &UncompressedFrameHeader) -> Self {
        let q = header.quant;
        Self {
            base_q_idx: q.base_q_idx,
            delta_q_y_dc: q.delta_q_y_dc,
            delta_q_u_dc: q.delta_q_u_dc,
            delta_q_u_ac: q.delta_q_u_ac,
            delta_q_v_dc: q.delta_q_v_dc,
            delta_q_v_ac: q.delta_q_v_ac,
            using_qmatrix: q.using_qmatrix,
            qm_y: q.qm_y,
            qm_u: q.qm_u,
            qm_v: q.qm_v,
        }
    }

    pub fn dequant_4x4(self, plane: Plane, coeffs_in: &[i16; 16], coeffs_out: &mut [i32; 16]) {
        let dc_q = dc_q_lookup_8bit(apply_delta(self.base_q_idx, self.dc_delta(plane)));
        let ac_q = ac_q_lookup_8bit(apply_delta(self.base_q_idx, self.ac_delta(plane)));
        coeffs_out[0] = i32::from(coeffs_in[0]) * i32::from(dc_q);
        for i in 1..16 {
            coeffs_out[i] = i32::from(coeffs_in[i]) * i32::from(ac_q);
        }
    }

    fn dc_delta(self, plane: Plane) -> i8 {
        match plane {
            Plane::Y => self.delta_q_y_dc,
            Plane::U => self.delta_q_u_dc,
            Plane::V => self.delta_q_v_dc,
        }
    }

    fn ac_delta(self, plane: Plane) -> i8 {
        match plane {
            Plane::Y => 0,
            Plane::U => self.delta_q_u_ac,
            Plane::V => self.delta_q_v_ac,
        }
    }
}

pub(crate) fn dequant_4x4(qindex: u8, coeffs_in: &[i16; 16], coeffs_out: &mut [i32; 16]) {
    let dc_q = dc_q_lookup_8bit(qindex);
    let ac_q = ac_q_lookup_8bit(qindex);
    coeffs_out[0] = i32::from(coeffs_in[0]) * i32::from(dc_q);
    for i in 1..16 {
        coeffs_out[i] = i32::from(coeffs_in[i]) * i32::from(ac_q);
    }
}

fn apply_delta(base_q_idx: u8, delta: i8) -> u8 {
    base_q_idx.saturating_add_signed(delta)
}

fn dc_q_lookup_8bit(qindex: u8) -> i16 {
    ac_q_lookup_8bit(qindex)
}

fn ac_q_lookup_8bit(qindex: u8) -> i16 {
    if qindex == 0 {
        return 64;
    }

    const BASE: [i16; 25] = [
        64, 40, 41, 43, 44, 45, 47, 48, 49, 51, 52, 54, 55, 57, 59, 60, 62, 64, 66, 68, 70,
        72, 74, 76, 78,
    ];

    if qindex < 25 {
        return BASE[qindex as usize];
    }

    BASE[((qindex - 1) % 24 + 1) as usize] << ((qindex - 1) / 24)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitstream::{
        CdefParams, DeltaLfParams, DeltaQParams, FilmGrainParams, FrameType, LoopFilterParams,
        LoopRestorationParams, QuantParams, RenderSize, SegmentationParams, SuperresParams,
        UncompressedFrameHeader,
    };

    #[test]
    fn dequant_4x4_scales_dc_and_ac_separately() {
        let coeffs_in = [1i16; 16];
        let mut out = [0i32; 16];
        dequant_4x4(10, &coeffs_in, &mut out);
        assert_eq!(out[0], i32::from(dc_q_lookup_8bit(10)));
        assert_eq!(out[1], i32::from(ac_q_lookup_8bit(10)));
    }

    #[test]
    fn ac_q_lookup_matches_reference_shape() {
        assert_eq!(ac_q_lookup_8bit(0), 64);
        assert_eq!(ac_q_lookup_8bit(1), 40);
        assert_eq!(ac_q_lookup_8bit(24), 78);
        assert_eq!(ac_q_lookup_8bit(25), 80);
        assert_eq!(ac_q_lookup_8bit(48), 156);
    }

    #[test]
    fn quant_context_uses_plane_deltas() {
        let header = UncompressedFrameHeader {
            frame_type: FrameType::Key,
            show_frame: true,
            error_resilient_mode: true,
            disable_cdf_update: false,
            primary_ref_frame: 0,
            refresh_frame_flags: 0xff,
            frame_size_override_flag: false,
            allow_screen_content_tools: false,
            force_integer_mv: false,
            order_hint: 0,
            render_size: RenderSize {
                width: 64,
                height: 64,
            },
            superres: SuperresParams {
                enabled: false,
                denominator: 8,
            },
            loop_filter: LoopFilterParams {
                level: [0; 4],
                sharpness: 0,
                delta_enabled: false,
                delta_update: false,
            },
            quant: QuantParams {
                base_q_idx: 20,
                delta_q_y_dc: -1,
                delta_q_u_dc: 2,
                delta_q_u_ac: 3,
                delta_q_v_dc: 4,
                delta_q_v_ac: 5,
                using_qmatrix: false,
                qm_y: 0,
                qm_u: 0,
                qm_v: 0,
            },
            segmentation: SegmentationParams {
                enabled: false,
                update_map: false,
                temporal_update: false,
                update_data: false,
            },
            delta_q: DeltaQParams {
                present: false,
                scale: 0,
            },
            delta_lf: DeltaLfParams {
                present: false,
                scale: 0,
                multi: false,
            },
            loop_restoration: LoopRestorationParams { uses_lrf: false },
            tx_mode: 0,
            reduced_tx_set: false,
            cdef: CdefParams {
                damping: 0,
                bits: 0,
            },
            film_grain: FilmGrainParams { apply_grain: false },
            num_tile_cols: 1,
            num_tile_rows: 1,
            frame_width: 64,
            frame_height: 64,
        };

        let q = QuantContext::from_frame_header(&header);
        let coeffs_in = [1i16; 16];
        let mut y = [0i32; 16];
        let mut u = [0i32; 16];
        q.dequant_4x4(Plane::Y, &coeffs_in, &mut y);
        q.dequant_4x4(Plane::U, &coeffs_in, &mut u);
        assert_eq!(y[0], i32::from(dc_q_lookup_8bit(19)));
        assert_eq!(u[0], i32::from(dc_q_lookup_8bit(22)));
        assert_eq!(u[1], i32::from(ac_q_lookup_8bit(23)));
    }
}
