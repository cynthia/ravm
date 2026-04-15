#![forbid(unsafe_code)]
//! Top-level decode loop; drives frame/tile/partition walks.

use crate::bitstream::{FrameType, SequenceHeader, TileGroup, UncompressedFrameHeader};
use crate::decoder::block_info::{BlockInfo, BlockInfoGrid};
use crate::decoder::entropy::{BacReader, EntropyError};
use crate::decoder::executor::{Sequential, TileExecutor};
use crate::decoder::frame_buffer::{FrameBuffer, PlaneBuffer};
use crate::decoder::intra::{
    predict_d45_4x4, predict_dc_4x4, predict_h_4x4, predict_paeth_4x4, predict_smooth_4x4,
    predict_smooth_h_4x4, predict_smooth_v_4x4, predict_v_4x4,
};
use crate::decoder::kernels;
use crate::decoder::partition::{partition_children, BlockSize};
use crate::decoder::quant::{Plane, QuantContext};
use crate::decoder::symbols::{PartitionType, TileContext};
use crate::decoder::transform::{
    base_intra_mode_from_actual_mode, default_tx_type_for_base_intra_mode, inverse_transform,
    TxSize, TxType,
};
use crate::format::Subsampling;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoreDecodeError {
    Unsupported(&'static str),
    UnexpectedMode,
    EntropyError,
}

pub(crate) fn decode_frame(
    sequence_header: SequenceHeader,
    frame_header: UncompressedFrameHeader,
    tile_group: &TileGroup<'_>,
) -> Result<FrameBuffer<u8>, CoreDecodeError> {
    if sequence_header.bit_depth != 8 {
        return Err(CoreDecodeError::Unsupported(
            "walking skeleton only supports 8-bit streams",
        ));
    }
    if sequence_header.monochrome
        || sequence_header.subsampling_x != 1
        || sequence_header.subsampling_y != 1
    {
        return Err(CoreDecodeError::Unsupported(
            "walking skeleton only supports 4:2:0 color streams",
        ));
    }
    if sequence_header.max_frame_width % 4 != 0 || sequence_header.max_frame_height % 4 != 0 {
        return Err(CoreDecodeError::Unsupported(
            "walking skeleton requires frame dimensions divisible by 4",
        ));
    }
    if frame_header.frame_type != FrameType::Key {
        return Err(CoreDecodeError::Unsupported(
            "walking skeleton only supports key frames",
        ));
    }
    if frame_header.num_tile_cols != 1 || frame_header.num_tile_rows != 1 {
        return Err(CoreDecodeError::Unsupported(
            "walking skeleton only supports single-tile frames",
        ));
    }
    if tile_group.data.is_empty() {
        return Err(CoreDecodeError::Unsupported(
            "walking skeleton requires a non-empty tile group payload",
        ));
    }

    let mut frame = FrameBuffer::<u8>::new(
        frame_header.frame_width as usize,
        frame_header.frame_height as usize,
        Subsampling::Yuv420,
    );
    let quant = QuantContext::from_frame_header(&frame_header);
    fill_plane(frame.chroma_u_mut(), 128);
    fill_plane(frame.chroma_v_mut(), 128);
    let exec = Sequential;
    let mut result = Ok(());
    exec.for_each_tile(1, |_| {
        if result.is_err() {
            return;
        }
        if let Err(err) = decode_tile(
            tile_group.data,
            &mut frame,
            quant,
            frame_header.disable_cdf_update,
            frame_header.tx_mode,
        ) {
            result = Err(err);
        }
    });
    result?;
    Ok(frame)
}

fn decode_tile(
    tile_data: &[u8],
    fb: &mut FrameBuffer<u8>,
    quant: QuantContext,
    disable_cdf_update: bool,
    tx_mode: u8,
) -> Result<(), CoreDecodeError> {
    let kernels = kernels::detect();
    let mut reader = BacReader::new(tile_data);
    let mut tile_ctx = if disable_cdf_update {
        TileContext::new(false)
    } else {
        TileContext::new_default()
    };
    let mut block_info = BlockInfoGrid::new(fb.luma().width, fb.luma().height);
    decode_partition(
        &mut reader,
        &mut tile_ctx,
        kernels,
        fb,
        &mut block_info,
        0,
        0,
        BlockSize::SB_M0,
        quant,
        tx_mode,
    )
}

fn decode_partition(
    reader: &mut BacReader<'_>,
    tile_ctx: &mut TileContext,
    kernels: &dyn kernels::Kernels,
    fb: &mut FrameBuffer<u8>,
    block_info: &mut BlockInfoGrid,
    bx: usize,
    by: usize,
    bsize: BlockSize,
    quant: QuantContext,
    tx_mode: u8,
) -> Result<(), CoreDecodeError> {
    if bx >= fb.luma().width || by >= fb.luma().height {
        return Ok(());
    }

    if bsize.is_min() {
        return decode_4x4_block(reader, tile_ctx, kernels, fb, block_info, bx, by, quant, tx_mode);
    }

    let ctx = block_info.partition_ctx(bx, by, bsize);
    let partition = reader.read_partition(tile_ctx, bsize, ctx);
    if partition == PartitionType::None {
        return decode_none_block(fb, block_info, bx, by, bsize);
    }

    for (child_x, child_y, child_size) in partition_children(bx, by, bsize, partition) {
        decode_partition(
            reader,
            tile_ctx,
            kernels,
            fb,
            block_info,
            child_x,
            child_y,
            child_size,
            quant,
            tx_mode,
        )?;
    }
    Ok(())
}

fn decode_none_block(
    fb: &mut FrameBuffer<u8>,
    block_info: &mut BlockInfoGrid,
    bx: usize,
    by: usize,
    bsize: BlockSize,
) -> Result<(), CoreDecodeError> {
    if bsize.is_min() {
        return Err(CoreDecodeError::UnexpectedMode);
    }

    let luma_width = fb.luma().width;
    let luma_height = fb.luma().height;
    let fill_width = bsize.width.min(luma_width.saturating_sub(bx));
    let fill_height = bsize.height.min(luma_height.saturating_sub(by));

    for y in 0..fill_height {
        let row = fb.luma_mut().row_mut(by + y);
        row[bx..bx + fill_width].fill(128);
    }

    block_info.fill_region(
        bx,
        by,
        bsize,
        BlockInfo {
            present: true,
            intra_mode: 0,
            skip: false,
            tx_size: 0,
        },
    );
    Ok(())
}

fn decode_4x4_block(
    reader: &mut BacReader<'_>,
    tile_ctx: &mut TileContext,
    kernels: &dyn kernels::Kernels,
    fb: &mut FrameBuffer<u8>,
    block_info: &mut BlockInfoGrid,
    bx: usize,
    by: usize,
    quant: QuantContext,
    tx_mode: u8,
) -> Result<(), CoreDecodeError> {
    let y_mode_ctx = block_info.y_mode_ctx(bx, by);
    let y_mode_list = block_info.y_intra_mode_list(bx, by, BlockSize::MIN);
    let intra_mode = reader.read_intra_mode(tile_ctx, y_mode_ctx, &y_mode_list);
    if tx_mode != 0 {
        return Err(CoreDecodeError::Unsupported(
            "tx_mode select is not integrated into the 4x4 Rust decode path yet",
        ));
    }
    let base_intra_mode = base_intra_mode_from_actual_mode(intra_mode.actual_mode)
        .ok_or(CoreDecodeError::UnexpectedMode)?;
    let tx_type = default_tx_type_for_base_intra_mode(base_intra_mode);
    if !matches!(
        tx_type,
        TxType::DctDct | TxType::Idtx | TxType::AdstDct | TxType::DctAdst | TxType::AdstAdst
    ) {
        return Err(CoreDecodeError::UnexpectedMode);
    }

    let above = if by >= 4 {
        Some(gather_above_4(fb.luma(), bx, by))
    } else {
        None
    };
    let above_wide = if by >= 4 {
        Some(gather_above_8(fb.luma(), bx, by))
    } else {
        None
    };
    let left = if bx >= 4 {
        Some(gather_left_4(fb.luma(), bx, by))
    } else {
        None
    };
    let above_left = if bx >= 4 && by >= 4 {
        Some(fb.luma().row(by - 1)[bx - 1])
    } else {
        None
    };

    let mut pred = [0u8; 16];
    match base_intra_mode {
        crate::decoder::transform::BaseIntraMode::Dc => {
            predict_dc_4x4(above.as_ref(), left.as_ref(), &mut pred, 4)
        }
        crate::decoder::transform::BaseIntraMode::V => {
            predict_v_4x4(above.as_ref(), &mut pred, 4)
        }
        crate::decoder::transform::BaseIntraMode::H => {
            predict_h_4x4(left.as_ref(), &mut pred, 4)
        }
        crate::decoder::transform::BaseIntraMode::D45 => {
            predict_d45_4x4(above_wide.as_ref(), &mut pred, 4)
        }
        crate::decoder::transform::BaseIntraMode::Smooth => {
            predict_smooth_4x4(above.as_ref(), left.as_ref(), &mut pred, 4)
        }
        crate::decoder::transform::BaseIntraMode::SmoothV => {
            predict_smooth_v_4x4(above.as_ref(), left.as_ref(), &mut pred, 4)
        }
        crate::decoder::transform::BaseIntraMode::SmoothH => {
            predict_smooth_h_4x4(above.as_ref(), left.as_ref(), &mut pred, 4)
        }
        crate::decoder::transform::BaseIntraMode::Paeth => {
            predict_paeth_4x4(above.as_ref(), left.as_ref(), above_left, &mut pred, 4)
        }
        _ => return Err(CoreDecodeError::UnexpectedMode),
    }

    let mut coeffs_in = [0i16; 16];
    reader
        .read_coeffs_4x4(tile_ctx, &mut coeffs_in)
        .map_err(map_entropy_error)?;
    let mut coeffs_out = [0i32; 16];
    quant.dequant_4x4(Plane::Y, &coeffs_in, &mut coeffs_out);
    let mut residual = [0i16; 16];
    inverse_transform(kernels, TxSize::Tx4x4, tx_type, &coeffs_out, &mut residual, 4);

    for y in 0..4 {
        if by + y >= fb.luma().height {
            continue;
        }
        let luma_width = fb.luma().width;
        let row = fb.luma_mut().row_mut(by + y);
        for x in 0..4 {
            if bx + x >= luma_width {
                continue;
            }
            let sample = i32::from(pred[y * 4 + x]) + i32::from(residual[y * 4 + x]);
            row[bx + x] = sample.clamp(0, 255) as u8;
        }
    }

    block_info.fill_region(
        bx,
        by,
        BlockSize::MIN,
        BlockInfo {
            present: true,
            intra_mode: intra_mode.joint_mode,
            skip: false,
            tx_size: 0,
        },
    );
    Ok(())
}

fn gather_above_4(plane: &PlaneBuffer<u8>, bx: usize, by: usize) -> [u8; 4] {
    let row = plane.row(by - 1);
    [row[bx], row[bx + 1], row[bx + 2], row[bx + 3]]
}

fn gather_above_8(plane: &PlaneBuffer<u8>, bx: usize, by: usize) -> [u8; 8] {
    let row = plane.row(by - 1);
    let last_x = plane.width.saturating_sub(1);
    [
        row[bx.min(last_x)],
        row[(bx + 1).min(last_x)],
        row[(bx + 2).min(last_x)],
        row[(bx + 3).min(last_x)],
        row[(bx + 4).min(last_x)],
        row[(bx + 5).min(last_x)],
        row[(bx + 6).min(last_x)],
        row[(bx + 7).min(last_x)],
    ]
}

fn gather_left_4(plane: &PlaneBuffer<u8>, bx: usize, by: usize) -> [u8; 4] {
    [
        plane.row(by)[bx - 1],
        plane.row(by + 1)[bx - 1],
        plane.row(by + 2)[bx - 1],
        plane.row(by + 3)[bx - 1],
    ]
}

fn fill_plane(plane: &mut PlaneBuffer<u8>, value: u8) {
    for y in 0..plane.height {
        plane.row_mut(y).fill(value);
    }
}

fn map_entropy_error(err: EntropyError) -> CoreDecodeError {
    match err {
        EntropyError::UnimplementedInM0 => CoreDecodeError::EntropyError,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitstream::{SequenceHeader, TileGroup, UncompressedFrameHeader};

    #[test]
    fn core_shell_allocates_midgray_luma() {
        let sh = SequenceHeader {
            profile: 0,
            still_picture: true,
            single_picture_header_flag: true,
            reduced_still_picture_header: true,
            timing_info_present_flag: false,
            initial_display_delay_present_flag: false,
            frame_id_numbers_present_flag: false,
            operating_points_cnt_minus_1: 0,
            operating_point_idc_0: 0,
            seq_level_idx_0: 0,
            seq_tier_0: None,
            max_frame_width: 64,
            max_frame_height: 48,
            bit_depth: 8,
            monochrome: false,
            subsampling_x: 1,
            subsampling_y: 1,
            color_range: 0,
            chroma_sample_position: 0,
            force_screen_content_tools: 0,
            force_integer_mv: 2,
            enable_intra_edge_filter: false,
            enable_cdef: false,
            enable_restoration: false,
            separate_uv_delta_q: false,
            equal_ac_dc_q: false,
            base_y_dc_delta_q: 0,
            y_dc_delta_q_enabled: false,
            base_uv_dc_delta_q: 0,
            uv_dc_delta_q_enabled: false,
            base_uv_ac_delta_q: 0,
            uv_ac_delta_q_enabled: false,
            reduced_tx_part_set: false,
            film_grain_params_present: false,
            num_bits_width: 8,
            num_bits_height: 8,
            df_par_bits_minus2: 0,
        };
        let fh = UncompressedFrameHeader {
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
            render_size: crate::bitstream::RenderSize {
                width: 64,
                height: 48,
            },
            superres: crate::bitstream::SuperresParams {
                enabled: false,
                denominator: 8,
            },
            loop_filter: crate::bitstream::LoopFilterParams {
                level: [0; 4],
                sharpness: 0,
                delta_enabled: false,
                delta_update: false,
            },
            quant: crate::bitstream::QuantParams {
                base_q_idx: 16,
                delta_q_y_dc: 0,
                delta_q_u_dc: 0,
                delta_q_u_ac: 0,
                delta_q_v_dc: 0,
                delta_q_v_ac: 0,
                using_qmatrix: false,
                qm_y: 0,
                qm_u: 0,
                qm_v: 0,
            },
            segmentation: crate::bitstream::SegmentationParams {
                enabled: false,
                update_map: false,
                temporal_update: false,
                update_data: false,
            },
            delta_q: crate::bitstream::DeltaQParams {
                present: false,
                scale: 0,
            },
            delta_lf: crate::bitstream::DeltaLfParams {
                present: false,
                scale: 0,
                multi: false,
            },
            loop_restoration: crate::bitstream::LoopRestorationParams { uses_lrf: false },
            tx_mode: 0,
            reduced_tx_set: false,
            cdef: crate::bitstream::CdefParams {
                damping: 0,
                bits: 0,
            },
            film_grain: crate::bitstream::FilmGrainParams { apply_grain: false },
            num_tile_cols: 1,
            num_tile_rows: 1,
            frame_width: 64,
            frame_height: 48,
        };
        let tg = TileGroup {
            tile_start: 0,
            tile_end: 0,
            data: &[0u8; 1],
        };

        let frame = decode_frame(sh, fh, &tg).expect("core shell decode");
        assert_eq!(frame.luma().width, 64);
        assert_eq!(frame.luma().height, 48);
        assert_eq!(frame.luma().row(0)[0], 128);
        assert_eq!(frame.luma().row(0)[1], 128);
    }

    #[test]
    fn core_shell_accepts_general_keyframe_headers() {
        let sh = SequenceHeader {
            profile: 0,
            still_picture: false,
            single_picture_header_flag: false,
            reduced_still_picture_header: false,
            timing_info_present_flag: false,
            initial_display_delay_present_flag: false,
            frame_id_numbers_present_flag: false,
            operating_points_cnt_minus_1: 0,
            operating_point_idc_0: 0,
            seq_level_idx_0: 0,
            seq_tier_0: None,
            max_frame_width: 64,
            max_frame_height: 48,
            bit_depth: 8,
            monochrome: false,
            subsampling_x: 1,
            subsampling_y: 1,
            color_range: 0,
            chroma_sample_position: 0,
            force_screen_content_tools: 0,
            force_integer_mv: 2,
            enable_intra_edge_filter: false,
            enable_cdef: false,
            enable_restoration: false,
            separate_uv_delta_q: false,
            equal_ac_dc_q: false,
            base_y_dc_delta_q: 0,
            y_dc_delta_q_enabled: false,
            base_uv_dc_delta_q: 0,
            uv_dc_delta_q_enabled: false,
            base_uv_ac_delta_q: 0,
            uv_ac_delta_q_enabled: false,
            reduced_tx_part_set: false,
            film_grain_params_present: false,
            num_bits_width: 8,
            num_bits_height: 8,
            df_par_bits_minus2: 0,
        };
        let fh = UncompressedFrameHeader {
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
            render_size: crate::bitstream::RenderSize {
                width: 64,
                height: 48,
            },
            superres: crate::bitstream::SuperresParams {
                enabled: false,
                denominator: 8,
            },
            loop_filter: crate::bitstream::LoopFilterParams {
                level: [0; 4],
                sharpness: 0,
                delta_enabled: false,
                delta_update: false,
            },
            quant: crate::bitstream::QuantParams {
                base_q_idx: 0,
                delta_q_y_dc: 0,
                delta_q_u_dc: 0,
                delta_q_u_ac: 0,
                delta_q_v_dc: 0,
                delta_q_v_ac: 0,
                using_qmatrix: false,
                qm_y: 0,
                qm_u: 0,
                qm_v: 0,
            },
            segmentation: crate::bitstream::SegmentationParams {
                enabled: false,
                update_map: false,
                temporal_update: false,
                update_data: false,
            },
            delta_q: crate::bitstream::DeltaQParams {
                present: false,
                scale: 0,
            },
            delta_lf: crate::bitstream::DeltaLfParams {
                present: false,
                scale: 0,
                multi: false,
            },
            loop_restoration: crate::bitstream::LoopRestorationParams { uses_lrf: false },
            tx_mode: 0,
            reduced_tx_set: false,
            cdef: crate::bitstream::CdefParams {
                damping: 0,
                bits: 0,
            },
            film_grain: crate::bitstream::FilmGrainParams { apply_grain: false },
            num_tile_cols: 1,
            num_tile_rows: 1,
            frame_width: 64,
            frame_height: 48,
        };
        let tg = TileGroup {
            tile_start: 0,
            tile_end: 0,
            data: &[0u8; 1],
        };

        let frame = decode_frame(sh, fh, &tg).expect("core shell decode");
        assert_eq!(frame.luma().width, 64);
        assert_eq!(frame.luma().height, 48);
        assert_eq!(frame.luma().row(0)[0], 128);
    }
}
