//! Minimal AV2/AV1-style OBU packet parsing.
//!
//! This module intentionally stops at packet structure. It does not attempt to
//! decode frame syntax yet; its job is to validate and split compressed payloads
//! into OBUs so the Rust backend can make deterministic progress before full
//! reconstruction exists.

use core::fmt;

/// Parsed OBU header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObuHeader {
    /// Raw OBU type field.
    pub obu_type: u8,
    /// Whether an extension header follows.
    pub has_extension: bool,
    /// Whether the OBU explicitly carries a payload size field.
    pub has_size_field: bool,
    /// Parsed extension header, if present.
    pub extension: Option<ObuExtensionHeader>,
}

/// Parsed OBU extension header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObuExtensionHeader {
    pub temporal_id: u8,
    pub spatial_id: u8,
}

/// Borrowed OBU view into the source packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Obu<'a> {
    pub header: ObuHeader,
    pub payload: &'a [u8],
}

/// Minimal sequence header information currently extracted by the pure-Rust backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SequenceHeader {
    pub profile: u8,
    pub still_picture: bool,
    pub single_picture_header_flag: bool,
    pub reduced_still_picture_header: bool,
    pub timing_info_present_flag: bool,
    pub initial_display_delay_present_flag: bool,
    pub frame_id_numbers_present_flag: bool,
    pub operating_points_cnt_minus_1: u8,
    pub operating_point_idc_0: u16,
    pub seq_level_idx_0: u8,
    pub seq_tier_0: Option<bool>,
    pub max_frame_width: u32,
    pub max_frame_height: u32,
    pub bit_depth: u8,
    pub monochrome: bool,
    pub subsampling_x: u8,
    pub subsampling_y: u8,
    pub color_range: u8,
    pub chroma_sample_position: u8,
    pub force_screen_content_tools: u8,
    pub force_integer_mv: u8,
    pub enable_intra_edge_filter: bool,
    pub enable_cdef: bool,
    pub enable_restoration: bool,
    pub separate_uv_delta_q: bool,
    pub equal_ac_dc_q: bool,
    pub base_y_dc_delta_q: i8,
    pub y_dc_delta_q_enabled: bool,
    pub base_uv_dc_delta_q: i8,
    pub uv_dc_delta_q_enabled: bool,
    pub base_uv_ac_delta_q: i8,
    pub uv_ac_delta_q_enabled: bool,
    pub reduced_tx_part_set: bool,
    pub film_grain_params_present: bool,
    pub num_bits_width: u8,
    pub num_bits_height: u8,
    pub df_par_bits_minus2: u8,
}

/// High-level classification for packets that contain frame-bearing OBUs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FramePacketKind {
    Frame,
    FrameHeader,
    TileGroup,
    RedundantFrameHeader,
    TileList,
    Mixed,
}

/// Minimal semantic frame classification currently exposed by the Rust backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameType {
    Key,
    Inter,
    IntraOnly,
    Switch,
}

/// Minimal frame-header information extracted by the Rust backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameHeaderInfo {
    pub frame_type: Option<FrameType>,
    pub show_frame: bool,
    pub show_existing_frame: bool,
    pub existing_frame_idx: Option<u8>,
    pub error_resilient_mode: Option<bool>,
    pub disable_cdf_update: Option<bool>,
    pub primary_ref_frame: Option<u8>,
    pub refresh_frame_flags: Option<u8>,
    pub frame_size_override_flag: Option<bool>,
}

/// Narrow uncompressed frame-header representation for the walking skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UncompressedFrameHeader {
    pub frame_type: FrameType,
    pub show_frame: bool,
    pub error_resilient_mode: bool,
    pub disable_cdf_update: bool,
    pub primary_ref_frame: u8,
    pub refresh_frame_flags: u8,
    pub frame_size_override_flag: bool,
    pub allow_screen_content_tools: bool,
    pub force_integer_mv: bool,
    pub order_hint: u8,
    pub render_size: RenderSize,
    pub superres: SuperresParams,
    pub loop_filter: LoopFilterParams,
    pub quant: QuantParams,
    pub segmentation: SegmentationParams,
    pub delta_q: DeltaQParams,
    pub delta_lf: DeltaLfParams,
    pub loop_restoration: LoopRestorationParams,
    pub tx_mode: u8,
    pub reduced_tx_set: bool,
    pub cdef: CdefParams,
    pub film_grain: FilmGrainParams,
    pub num_tile_cols: usize,
    pub num_tile_rows: usize,
    pub frame_width: u32,
    pub frame_height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RenderSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SuperresParams {
    pub enabled: bool,
    pub denominator: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoopFilterParams {
    pub level: [u8; 4],
    pub sharpness: u8,
    pub delta_enabled: bool,
    pub delta_update: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QuantParams {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SegmentationParams {
    pub enabled: bool,
    pub update_map: bool,
    pub temporal_update: bool,
    pub update_data: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeltaQParams {
    pub present: bool,
    pub scale: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeltaLfParams {
    pub present: bool,
    pub scale: u8,
    pub multi: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LoopRestorationParams {
    pub uses_lrf: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CdefParams {
    pub damping: u8,
    pub bits: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FilmGrainParams {
    pub apply_grain: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SequenceToolset {
    force_screen_content_tools: u8,
    force_integer_mv: u8,
    enable_intra_edge_filter: bool,
    enable_cdef: bool,
    enable_restoration: bool,
    separate_uv_delta_q: bool,
    equal_ac_dc_q: bool,
    base_y_dc_delta_q: i8,
    y_dc_delta_q_enabled: bool,
    base_uv_dc_delta_q: i8,
    uv_dc_delta_q_enabled: bool,
    base_uv_ac_delta_q: i8,
    uv_ac_delta_q_enabled: bool,
    reduced_tx_part_set: bool,
    film_grain_params_present: bool,
    df_par_bits_minus2: u8,
}

impl SequenceToolset {
    const fn defaults(_monochrome: bool, single_picture_header_flag: bool) -> Self {
        Self {
            force_screen_content_tools: if single_picture_header_flag { 2 } else { 0 },
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
            df_par_bits_minus2: 0,
        }
    }
}

/// Parsed tile-group payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileGroup<'a> {
    pub tile_start: usize,
    pub tile_end: usize,
    pub data: &'a [u8],
}

/// Bitstream packet parse error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error("truncated OBU header")]
    TruncatedHeader,
    #[error("invalid OBU header")]
    InvalidHeader,
    #[error("truncated OBU extension header")]
    TruncatedExtension,
    #[error("invalid OBU extension header")]
    InvalidExtension,
    #[error("truncated OBU size field")]
    TruncatedSize,
    #[error("OBU size field overflow")]
    SizeOverflow,
    #[error("OBU payload exceeds packet length")]
    TruncatedPayload,
    #[error("truncated sequence header")]
    TruncatedSequenceHeader,
    #[error("unsupported sequence header form")]
    UnsupportedSequenceHeader,
    #[error("truncated frame header")]
    TruncatedFrameHeader,
    #[error("unsupported frame header form")]
    UnsupportedFrameHeader,
    #[error("truncated tile group")]
    TruncatedTileGroup,
}

/// OBU types we recognize at the packet layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObuType {
    SequenceHeader,
    TemporalDelimiter,
    FrameHeader,
    TileGroup,
    Metadata,
    Frame,
    RedundantFrameHeader,
    TileList,
    Padding,
    Other(u8),
}

impl ObuType {
    pub fn from_raw(raw: u8) -> Self {
        match raw {
            1 => Self::SequenceHeader,
            2 => Self::TemporalDelimiter,
            3 => Self::FrameHeader,
            4 | 10 | 11 | 12 | 13 | 14 | 19 | 21 => Self::Frame,
            6 | 7 => Self::TileGroup,
            5 | 8 | 9 => Self::Metadata,
            15 => Self::Padding,
            16 | 17 | 18 | 20 | 22 | 23 | 24 => Self::Other(raw),
            other => Self::Other(other),
        }
    }

    pub fn is_frame_data(self) -> bool {
        matches!(
            self,
            Self::FrameHeader
                | Self::TileGroup
                | Self::Frame
                | Self::RedundantFrameHeader
                | Self::TileList
        )
    }
}

/// Classify the frame-bearing intent of one parsed packet.
pub fn classify_frame_packet(obus: &[Obu<'_>]) -> Option<FramePacketKind> {
    let mut kind = None;
    for obu in obus {
        let current = match ObuType::from_raw(obu.header.obu_type) {
            ObuType::Frame => Some(FramePacketKind::Frame),
            ObuType::FrameHeader => Some(FramePacketKind::FrameHeader),
            ObuType::TileGroup => Some(FramePacketKind::TileGroup),
            ObuType::RedundantFrameHeader => Some(FramePacketKind::RedundantFrameHeader),
            ObuType::TileList => Some(FramePacketKind::TileList),
            _ => None,
        };

        if let Some(current) = current {
            kind = Some(match kind {
                None => current,
                Some(existing) if existing == current => existing,
                Some(_) => FramePacketKind::Mixed,
            });
        }
    }
    kind
}

/// Derive frame-header semantics for reduced-still-picture mode.
pub fn reduced_still_picture_frame_header(
    sequence_header: &SequenceHeader,
    packet_kind: FramePacketKind,
) -> Option<FrameHeaderInfo> {
    if !sequence_header.reduced_still_picture_header {
        return None;
    }

    match packet_kind {
        FramePacketKind::Frame | FramePacketKind::FrameHeader | FramePacketKind::Mixed => {
            Some(FrameHeaderInfo {
                frame_type: Some(FrameType::Key),
                show_frame: true,
                show_existing_frame: false,
                existing_frame_idx: None,
                error_resilient_mode: Some(true),
                disable_cdf_update: None,
                primary_ref_frame: None,
                refresh_frame_flags: None,
                frame_size_override_flag: None,
            })
        }
        FramePacketKind::TileGroup
        | FramePacketKind::RedundantFrameHeader
        | FramePacketKind::TileList => None,
    }
}

fn parse_frame_type(raw: u32) -> Option<FrameType> {
    match raw {
        0 => Some(FrameType::Key),
        1 => Some(FrameType::Inter),
        2 => Some(FrameType::IntraOnly),
        3 => Some(FrameType::Switch),
        _ => None,
    }
}

/// Parse a narrow frame-header subset.
pub fn parse_frame_header_info(
    sequence_header: &SequenceHeader,
    obu_type: ObuType,
    payload: &[u8],
) -> Result<Option<FrameHeaderInfo>, ParseError> {
    if !matches!(obu_type, ObuType::Frame | ObuType::FrameHeader) {
        return Ok(None);
    }

    if sequence_header.reduced_still_picture_header {
        return Ok(Some(FrameHeaderInfo {
            frame_type: Some(FrameType::Key),
            show_frame: true,
            show_existing_frame: false,
            existing_frame_idx: None,
            error_resilient_mode: Some(true),
            disable_cdf_update: None,
            primary_ref_frame: None,
            refresh_frame_flags: None,
            frame_size_override_flag: None,
        }));
    }

    let mut bits = BitReader::new(payload);
    let show_existing_frame = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    if show_existing_frame {
        let existing_frame_idx =
            bits.read_bits_with(3, ParseError::TruncatedFrameHeader)? as u8;
        return Ok(Some(FrameHeaderInfo {
            frame_type: None,
            show_frame: true,
            show_existing_frame: true,
            existing_frame_idx: Some(existing_frame_idx),
            error_resilient_mode: None,
            disable_cdf_update: None,
            primary_ref_frame: None,
            refresh_frame_flags: None,
            frame_size_override_flag: None,
        }));
    }

    let frame_type = parse_frame_type(bits.read_bits_with(2, ParseError::TruncatedFrameHeader)?)
        .ok_or(ParseError::TruncatedFrameHeader)?;
    let show_frame = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    let error_resilient_mode = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    let disable_cdf_update = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    let primary_ref_frame = bits.read_bits_with(3, ParseError::TruncatedFrameHeader)? as u8;
    let refresh_frame_flags = bits.read_bits_with(8, ParseError::TruncatedFrameHeader)? as u8;
    let frame_size_override_flag = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    Ok(Some(FrameHeaderInfo {
        frame_type: Some(frame_type),
        show_frame,
        show_existing_frame: false,
        existing_frame_idx: None,
        error_resilient_mode: Some(error_resilient_mode),
        disable_cdf_update: Some(disable_cdf_update),
        primary_ref_frame: Some(primary_ref_frame),
        refresh_frame_flags: Some(refresh_frame_flags),
        frame_size_override_flag: Some(frame_size_override_flag),
    }))
}

/// Parse a narrow KEY-frame-only uncompressed frame header.
pub fn parse_uncompressed_frame_header(
    sequence_header: &SequenceHeader,
    payload: &[u8],
) -> Result<UncompressedFrameHeader, ParseError> {
    Ok(split_frame_obu_payload(sequence_header, payload)?.0)
}

/// Parse a narrow KEY-frame-only uncompressed frame header and return the
/// remaining tile payload bytes from a `Frame` OBU.
pub fn split_frame_obu_payload<'a>(
    sequence_header: &SequenceHeader,
    payload: &'a [u8],
) -> Result<(UncompressedFrameHeader, &'a [u8]), ParseError> {
    let mut bits = BitReader::new(payload);

    if sequence_header.reduced_still_picture_header {
        let base_q_idx = bits.read_bits_with(8, ParseError::TruncatedFrameHeader)? as u8;
        let quant = parse_quant_params(sequence_header, &mut bits, base_q_idx)?;
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
                width: sequence_header.max_frame_width,
                height: sequence_header.max_frame_height,
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
            quant,
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
            frame_width: sequence_header.max_frame_width,
            frame_height: sequence_header.max_frame_height,
        };
        let tile_payload = &payload[bits.bytes_consumed()..];
        return Ok((header, tile_payload));
    }

    let show_existing_frame = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    if show_existing_frame {
        return Err(ParseError::UnsupportedFrameHeader);
    }

    let frame_type = parse_frame_type(bits.read_bits_with(2, ParseError::TruncatedFrameHeader)?)
        .ok_or(ParseError::TruncatedFrameHeader)?;
    if frame_type != FrameType::Key {
        return Err(ParseError::UnsupportedFrameHeader);
    }

    let show_frame = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    let error_resilient_mode = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    let disable_cdf_update = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    let primary_ref_frame = bits.read_bits_with(3, ParseError::TruncatedFrameHeader)? as u8;
    let refresh_frame_flags = bits.read_bits_with(8, ParseError::TruncatedFrameHeader)? as u8;
    let frame_size_override_flag = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    let (frame_width, frame_height) =
        read_frame_size(sequence_header, &mut bits, frame_size_override_flag)?;
    let allow_screen_content_tools = read_screen_content_tools(sequence_header, &mut bits)?;
    let force_integer_mv =
        read_force_integer_mv(sequence_header, &mut bits, allow_screen_content_tools)?;
    let base_q_idx = bits.read_bits_with(8, ParseError::TruncatedFrameHeader)? as u8;
    let quant = parse_quant_params(sequence_header, &mut bits, base_q_idx)?;
    let (loop_filter, segmentation, quant, delta_q, tx_mode, reduced_tx_set) =
        parse_general_keyframe_post_quant_subset(sequence_header, quant, &mut bits)?;

    let header = UncompressedFrameHeader {
        frame_type,
        show_frame,
        error_resilient_mode,
        disable_cdf_update,
        primary_ref_frame,
        refresh_frame_flags,
        frame_size_override_flag,
        allow_screen_content_tools,
        force_integer_mv,
        order_hint: 0,
        render_size: RenderSize {
            width: frame_width,
            height: frame_height,
        },
        superres: SuperresParams {
            enabled: false,
            denominator: 8,
        },
        loop_filter,
        quant,
        segmentation,
        delta_q,
        delta_lf: DeltaLfParams {
            present: false,
            scale: 0,
            multi: false,
        },
        loop_restoration: LoopRestorationParams { uses_lrf: false },
        tx_mode,
        reduced_tx_set,
        cdef: CdefParams {
            damping: 0,
            bits: 0,
        },
        film_grain: FilmGrainParams { apply_grain: false },
        num_tile_cols: 1,
        num_tile_rows: 1,
        frame_width,
        frame_height,
    };
    let tile_payload = &payload[bits.bytes_consumed()..];
    Ok((header, tile_payload))
}

fn parse_general_keyframe_post_quant_subset(
    sequence_header: &SequenceHeader,
    mut quant: QuantParams,
    bits: &mut BitReader<'_>,
) -> Result<
    (
        LoopFilterParams,
        SegmentationParams,
        QuantParams,
        DeltaQParams,
        u8,
        bool,
    ),
    ParseError,
> {
    let lossless = is_lossless_without_delta_q(sequence_header, &quant);
    let segmentation_enabled = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    if segmentation_enabled {
        return Err(ParseError::UnsupportedFrameHeader);
    }

    quant = parse_qmatrix_params_subset(sequence_header, quant, bits)?;

    let delta_q = if lossless || quant.base_q_idx == 0 {
        DeltaQParams {
            present: false,
            scale: 0,
        }
    } else {
        let present = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
        let scale = if present {
            1u8 << bits.read_bits_with(2, ParseError::TruncatedFrameHeader)? as u8
        } else {
            0
        };
        DeltaQParams { present, scale }
    };

    let loop_filter = if lossless {
        LoopFilterParams {
            level: [0; 4],
            sharpness: 0,
            delta_enabled: false,
            delta_update: false,
        }
    } else {
        parse_loop_filter_params_subset(sequence_header, bits)?
    };

    let tx_mode = if lossless {
        0
    } else if bits.read_bit_with(ParseError::TruncatedFrameHeader)? {
        1
    } else {
        0
    };
    let reduced_tx_set = bits.read_bits_with(2, ParseError::TruncatedFrameHeader)? != 0;

    Ok((
        loop_filter,
        SegmentationParams {
            enabled: false,
            update_map: false,
            temporal_update: false,
            update_data: false,
        },
        quant,
        delta_q,
        tx_mode,
        reduced_tx_set,
    ))
}

fn is_lossless_without_delta_q(sequence_header: &SequenceHeader, quant: &QuantParams) -> bool {
    quant.base_q_idx == 0
        && i16::from(quant.delta_q_y_dc) + i16::from(sequence_header.base_y_dc_delta_q) <= 0
        && i16::from(quant.delta_q_u_dc) + i16::from(sequence_header.base_uv_dc_delta_q) <= 0
        && i16::from(quant.delta_q_v_dc) + i16::from(sequence_header.base_uv_dc_delta_q) <= 0
        && i16::from(quant.delta_q_u_ac) + i16::from(sequence_header.base_uv_ac_delta_q) <= 0
        && i16::from(quant.delta_q_v_ac) + i16::from(sequence_header.base_uv_ac_delta_q) <= 0
}

fn parse_loop_filter_params_subset(
    sequence_header: &SequenceHeader,
    bits: &mut BitReader<'_>,
) -> Result<LoopFilterParams, ParseError> {
    let apply_h = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    let apply_v = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;

    let (apply_u, apply_v_plane) = if sequence_header.monochrome {
        (false, false)
    } else if apply_h || apply_v {
        (
            bits.read_bit_with(ParseError::TruncatedFrameHeader)?,
            bits.read_bit_with(ParseError::TruncatedFrameHeader)?,
        )
    } else {
        (false, false)
    };

    let df_par_bits = sequence_header.df_par_bits_minus2 + 2;
    let delta_luma_h = if apply_h {
        read_optional_signed_literal(bits, df_par_bits)?
    } else {
        0
    };
    let delta_luma_v = if apply_v {
        let same = !bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
        if same {
            delta_luma_h
        } else {
            read_signed_literal(bits, df_par_bits)?
        }
    } else {
        0
    };
    let delta_u = if apply_u {
        read_optional_signed_literal(bits, df_par_bits)?
    } else {
        0
    };
    let delta_v = if apply_v_plane {
        read_optional_signed_literal(bits, df_par_bits)?
    } else {
        0
    };

    Ok(LoopFilterParams {
        level: [
            u8::from(apply_h),
            u8::from(apply_v),
            u8::from(apply_u),
            u8::from(apply_v_plane),
        ],
        sharpness: 0,
        delta_enabled: delta_luma_h != 0 || delta_luma_v != 0 || delta_u != 0 || delta_v != 0,
        delta_update: apply_h || apply_v || apply_u || apply_v_plane,
    })
}

fn parse_qmatrix_params_subset(
    sequence_header: &SequenceHeader,
    mut quant: QuantParams,
    bits: &mut BitReader<'_>,
) -> Result<QuantParams, ParseError> {
    let using_qmatrix = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    if !using_qmatrix {
        return Ok(quant);
    }

    quant.using_qmatrix = true;
    quant.qm_y = bits.read_bits_with(4, ParseError::TruncatedFrameHeader)? as u8;
    if sequence_header.monochrome {
        quant.qm_u = 0;
        quant.qm_v = 0;
        return Ok(quant);
    }

    let qm_uv_same_as_y = bits.read_bit_with(ParseError::TruncatedFrameHeader)?;
    if qm_uv_same_as_y {
        quant.qm_u = quant.qm_y;
        quant.qm_v = quant.qm_y;
        return Ok(quant);
    }

    quant.qm_u = bits.read_bits_with(4, ParseError::TruncatedFrameHeader)? as u8;
    if sequence_header.separate_uv_delta_q {
        quant.qm_v = bits.read_bits_with(4, ParseError::TruncatedFrameHeader)? as u8;
    } else {
        quant.qm_v = quant.qm_u;
    }
    Ok(quant)
}

fn read_optional_signed_literal(bits: &mut BitReader<'_>, count: u8) -> Result<i16, ParseError> {
    if !bits.read_bit_with(ParseError::TruncatedFrameHeader)? {
        return Ok(0);
    }
    read_signed_literal(bits, count)
}

fn read_signed_literal(bits: &mut BitReader<'_>, count: u8) -> Result<i16, ParseError> {
    let offset = 1i16 << (count - 1);
    Ok(bits.read_bits_with(count, ParseError::TruncatedFrameHeader)? as i16 - offset)
}

fn read_frame_size(
    sequence_header: &SequenceHeader,
    bits: &mut BitReader<'_>,
    frame_size_override_flag: bool,
) -> Result<(u32, u32), ParseError> {
    if !frame_size_override_flag {
        return Ok((
            sequence_header.max_frame_width,
            sequence_header.max_frame_height,
        ));
    }

    let frame_width = bits.read_bits_with(
        sequence_header.num_bits_width,
        ParseError::TruncatedFrameHeader,
    )? + 1;
    let frame_height = bits.read_bits_with(
        sequence_header.num_bits_height,
        ParseError::TruncatedFrameHeader,
    )? + 1;

    if frame_width > sequence_header.max_frame_width
        || frame_height > sequence_header.max_frame_height
    {
        return Err(ParseError::UnsupportedFrameHeader);
    }

    Ok((frame_width, frame_height))
}

fn read_screen_content_tools(
    sequence_header: &SequenceHeader,
    bits: &mut BitReader<'_>,
) -> Result<bool, ParseError> {
    Ok(match sequence_header.force_screen_content_tools {
        0 => false,
        1 => true,
        2 => bits.read_bit_with(ParseError::TruncatedFrameHeader)?,
        _ => return Err(ParseError::UnsupportedFrameHeader),
    })
}

fn read_force_integer_mv(
    sequence_header: &SequenceHeader,
    bits: &mut BitReader<'_>,
    allow_screen_content_tools: bool,
) -> Result<bool, ParseError> {
    if !allow_screen_content_tools {
        return Ok(false);
    }
    Ok(match sequence_header.force_integer_mv {
        0 => false,
        1 => true,
        2 => bits.read_bit_with(ParseError::TruncatedFrameHeader)?,
        _ => return Err(ParseError::UnsupportedFrameHeader),
    })
}

fn parse_quant_params(
    sequence_header: &SequenceHeader,
    bits: &mut BitReader<'_>,
    base_q_idx: u8,
) -> Result<QuantParams, ParseError> {
    let delta_q_y_dc = i16::from(sequence_header.base_y_dc_delta_q)
        + i16::from(if sequence_header.y_dc_delta_q_enabled {
            read_delta_q(bits, ParseError::TruncatedFrameHeader)?
        } else {
            0
        });

    let (delta_q_u_dc, delta_q_u_ac, delta_q_v_dc, delta_q_v_ac) = if sequence_header.monochrome {
        (0, 0, 0, 0)
    } else {
        let diff_uv_delta = if sequence_header.separate_uv_delta_q {
            bits.read_bit_with(ParseError::TruncatedFrameHeader)?
        } else {
            false
        };

        let u_dc = i16::from(sequence_header.base_uv_dc_delta_q)
            + i16::from(if sequence_header.equal_ac_dc_q {
                0
            } else if sequence_header.uv_dc_delta_q_enabled {
                read_delta_q(bits, ParseError::TruncatedFrameHeader)?
            } else {
                0
            });
        let u_ac = i16::from(sequence_header.base_uv_ac_delta_q)
            + i16::from(if sequence_header.uv_ac_delta_q_enabled {
                read_delta_q(bits, ParseError::TruncatedFrameHeader)?
            } else {
                0
            });
        let u_dc = if sequence_header.equal_ac_dc_q { u_ac } else { u_dc };

        if diff_uv_delta {
            let v_dc = i16::from(sequence_header.base_uv_dc_delta_q)
                + i16::from(if sequence_header.equal_ac_dc_q {
                    0
                } else if sequence_header.uv_dc_delta_q_enabled {
                    read_delta_q(bits, ParseError::TruncatedFrameHeader)?
                } else {
                    0
                });
            let v_ac = i16::from(sequence_header.base_uv_ac_delta_q)
                + i16::from(if sequence_header.uv_ac_delta_q_enabled {
                    read_delta_q(bits, ParseError::TruncatedFrameHeader)?
                } else {
                    0
                });
            let v_dc = if sequence_header.equal_ac_dc_q { v_ac } else { v_dc };
            (
                clamp_i16_to_i8(u_dc),
                clamp_i16_to_i8(u_ac),
                clamp_i16_to_i8(v_dc),
                clamp_i16_to_i8(v_ac),
            )
        } else {
            let u_dc = clamp_i16_to_i8(u_dc);
            let u_ac = clamp_i16_to_i8(u_ac);
            (u_dc, u_ac, u_dc, u_ac)
        }
    };

    Ok(QuantParams {
        base_q_idx,
        delta_q_y_dc: clamp_i16_to_i8(delta_q_y_dc),
        delta_q_u_dc,
        delta_q_u_ac,
        delta_q_v_dc,
        delta_q_v_ac,
        using_qmatrix: false,
        qm_y: 0,
        qm_u: 0,
        qm_v: 0,
    })
}

fn read_delta_q(bits: &mut BitReader<'_>, err: ParseError) -> Result<i8, ParseError> {
    if !bits.read_bit_with(err)? {
        return Ok(0);
    }
    let raw = bits.read_bits_with(6, err)? as i16;
    let signed = if (raw & 0x20) != 0 { raw - 0x40 } else { raw };
    Ok(clamp_i16_to_i8(signed))
}

fn clamp_i16_to_i8(value: i16) -> i8 {
    value.clamp(i16::from(i8::MIN), i16::from(i8::MAX)) as i8
}

/// Parse a single-tile tile-group payload for the walking skeleton.
pub fn parse_tile_group<'a>(
    _sh: &SequenceHeader,
    fh: &UncompressedFrameHeader,
    payload: &'a [u8],
) -> Result<TileGroup<'a>, ParseError> {
    if fh.num_tile_cols != 1 || fh.num_tile_rows != 1 {
        return Err(ParseError::UnsupportedFrameHeader);
    }
    if payload.is_empty() {
        return Err(ParseError::TruncatedTileGroup);
    }
    Ok(TileGroup {
        tile_start: 0,
        tile_end: 0,
        data: payload,
    })
}

impl fmt::Display for ObuType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SequenceHeader => f.write_str("sequence_header"),
            Self::TemporalDelimiter => f.write_str("temporal_delimiter"),
            Self::FrameHeader => f.write_str("frame_header"),
            Self::TileGroup => f.write_str("tile_group"),
            Self::Metadata => f.write_str("metadata"),
            Self::Frame => f.write_str("frame"),
            Self::RedundantFrameHeader => f.write_str("redundant_frame_header"),
            Self::TileList => f.write_str("tile_list"),
            Self::Padding => f.write_str("padding"),
            Self::Other(raw) => write!(f, "other({raw})"),
        }
    }
}

struct BitReader<'a> {
    data: &'a [u8],
    bit_offset: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            bit_offset: 0,
        }
    }

    fn read_bit_with(&mut self, err: ParseError) -> Result<bool, ParseError> {
        let byte_index = self.bit_offset / 8;
        let bit_index = 7 - (self.bit_offset % 8);
        let Some(&byte) = self.data.get(byte_index) else {
            return Err(err);
        };
        self.bit_offset += 1;
        Ok(((byte >> bit_index) & 1) != 0)
    }

    fn read_bits_with(&mut self, count: u8, err: ParseError) -> Result<u32, ParseError> {
        let mut value = 0u32;
        for _ in 0..count {
            value = (value << 1) | u32::from(self.read_bit_with(err)?);
        }
        Ok(value)
    }

    fn read_uvlc(&mut self, err: ParseError) -> Result<u32, ParseError> {
        let mut leading_zeros = 0u32;
        while !self.read_bit_with(err)? {
            leading_zeros += 1;
            if leading_zeros > 31 {
                return Err(err);
            }
        }
        if leading_zeros == 0 {
            return Ok(0);
        }
        let suffix = self.read_bits_with(leading_zeros as u8, err)?;
        Ok(((1u32 << leading_zeros) - 1) + suffix)
    }

    fn bytes_consumed(&self) -> usize {
        self.bit_offset.div_ceil(8)
    }
}

fn parse_leb128(data: &[u8], offset: &mut usize) -> Result<usize, ParseError> {
    let mut value = 0usize;
    let mut shift = 0u32;
    loop {
        let Some(&byte) = data.get(*offset) else {
            return Err(ParseError::TruncatedSize);
        };
        *offset += 1;

        let low = usize::from(byte & 0x7f);
        if shift >= usize::BITS || (low.checked_shl(shift).is_none()) {
            return Err(ParseError::SizeOverflow);
        }
        value |= low << shift;

        if byte & 0x80 == 0 {
            return Ok(value);
        }

        shift += 7;
        if shift > (usize::BITS - 1) {
            return Err(ParseError::SizeOverflow);
        }
    }
}

/// Split a packet into OBU records.
pub fn parse_obus(mut data: &[u8]) -> Result<Vec<Obu<'_>>, ParseError> {
    let mut obus = Vec::new();

    while !data.is_empty() {
        let header_byte = *data.first().ok_or(ParseError::TruncatedHeader)?;
        data = &data[1..];

        if header_byte & 0x80 != 0 || header_byte & 0x01 != 0 {
            return Err(ParseError::InvalidHeader);
        }

        let obu_type = (header_byte >> 3) & 0x0f;
        let has_extension = (header_byte & 0x04) != 0;
        let has_size_field = (header_byte & 0x02) != 0;

        let extension = if has_extension {
            let Some((&ext, rest)) = data.split_first() else {
                return Err(ParseError::TruncatedExtension);
            };
            data = rest;
            if ext & 0x07 != 0 {
                return Err(ParseError::InvalidExtension);
            }
            Some(ObuExtensionHeader {
                temporal_id: (ext >> 5) & 0x07,
                spatial_id: (ext >> 3) & 0x03,
            })
        } else {
            None
        };

        let payload_len = if has_size_field {
            let mut size_offset = 0usize;
            let len = parse_leb128(data, &mut size_offset)?;
            data = &data[size_offset..];
            len
        } else {
            data.len()
        };

        if payload_len > data.len() {
            return Err(ParseError::TruncatedPayload);
        }

        let (payload, rest) = data.split_at(payload_len);
        obus.push(Obu {
            header: ObuHeader {
                obu_type,
                has_extension,
                has_size_field,
                extension,
            },
            payload,
        });
        data = rest;
    }

    Ok(obus)
}

fn parse_annexb_obu_header(data: &[u8]) -> Result<(ObuHeader, usize), ParseError> {
    let Some(&header_byte) = data.first() else {
        return Err(ParseError::TruncatedHeader);
    };

    let has_extension = (header_byte & 0x80) != 0;
    let obu_type = (header_byte >> 2) & 0x1f;
    let temporal_id = header_byte & 0x03;
    let mut header_len = 1usize;

    let extension = if has_extension {
        let Some(&ext) = data.get(1) else {
            return Err(ParseError::TruncatedExtension);
        };
        header_len += 1;
        Some(ObuExtensionHeader {
            temporal_id,
            spatial_id: ext >> 5,
        })
    } else {
        None
    };

    Ok((
        ObuHeader {
            obu_type,
            has_extension,
            has_size_field: false,
            extension,
        },
        header_len,
    ))
}

/// Split an Annex-B frame-unit packet into OBU records.
pub fn parse_annexb_obus(data: &[u8]) -> Result<Vec<Obu<'_>>, ParseError> {
    let mut obus = Vec::new();
    let mut offset = 0usize;

    while offset < data.len() {
        let obu_len = parse_leb128(data, &mut offset)?;
        let Some(obu_end) = offset.checked_add(obu_len) else {
            return Err(ParseError::SizeOverflow);
        };
        if obu_end > data.len() {
            return Err(ParseError::TruncatedPayload);
        }
        let obu = &data[offset..obu_end];
        let (header, header_len) = parse_annexb_obu_header(obu)?;
        let payload = &obu[header_len..];
        obus.push(Obu { header, payload });
        offset = obu_end;
    }

    Ok(obus)
}

/// Parse OBUs from either low-overhead packet form or Annex-B frame-unit form.
pub fn parse_obus_auto(data: &[u8]) -> Result<Vec<Obu<'_>>, ParseError> {
    parse_obus(data).or_else(|_| parse_annexb_obus(data))
}

/// Parse the subset of sequence header syntax currently implemented by the
/// Rust backend.
///
/// At this stage we intentionally support only the reduced-still-picture path,
/// because it is compact and enough to exercise real metadata extraction
/// without claiming full AV2 header coverage.
pub fn parse_sequence_header(payload: &[u8]) -> Result<SequenceHeader, ParseError> {
    parse_sequence_header_av2(payload).or_else(|_| parse_sequence_header_legacy(payload))
}

fn parse_sequence_header_legacy(payload: &[u8]) -> Result<SequenceHeader, ParseError> {
    let mut bits = BitReader::new(payload);
    let profile = bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as u8;
    let still_picture = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let reduced_still_picture_header =
        bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;

    let (timing_info_present_flag, initial_display_delay_present_flag, frame_id_numbers_present_flag, operating_points_cnt_minus_1, operating_point_idc_0, seq_level_idx_0, seq_tier_0, max_frame_width, max_frame_height, num_bits_width, num_bits_height) =
        if reduced_still_picture_header
    {
        let level = bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as u8;
        let seq_tier_0 = if level > 7 {
            Some(bits.read_bit_with(ParseError::TruncatedSequenceHeader)?)
        } else {
            None
        };

        let frame_width_bits_minus_1 =
            bits.read_bits_with(4, ParseError::TruncatedSequenceHeader)? as u8;
        let frame_height_bits_minus_1 =
            bits.read_bits_with(4, ParseError::TruncatedSequenceHeader)? as u8;
        let max_frame_width = bits
            .read_bits_with(frame_width_bits_minus_1 + 1, ParseError::TruncatedSequenceHeader)?
            + 1;
        let max_frame_height = bits
            .read_bits_with(frame_height_bits_minus_1 + 1, ParseError::TruncatedSequenceHeader)?
            + 1;
        (
            false,
            false,
            false,
            0,
            0,
            level,
            seq_tier_0,
            max_frame_width,
            max_frame_height,
            frame_width_bits_minus_1 + 1,
            frame_height_bits_minus_1 + 1,
        )
    } else {
        let timing_info_present_flag = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
        if timing_info_present_flag {
            return Err(ParseError::UnsupportedSequenceHeader);
        }
        let initial_display_delay_present_flag =
            bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
        if initial_display_delay_present_flag {
            return Err(ParseError::UnsupportedSequenceHeader);
        }

        let operating_points_cnt_minus_1 =
            bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as u8;
        let mut operating_point_idc_0 = None;
        let mut seq_level_idx_0 = None;
        let mut seq_tier_0 = None;
        for _ in 0..=operating_points_cnt_minus_1 {
            let operating_point_idc =
                bits.read_bits_with(12, ParseError::TruncatedSequenceHeader)? as u16;
            let level = bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as u8;
            let tier = if level > 7 {
                Some(bits.read_bit_with(ParseError::TruncatedSequenceHeader)?)
            } else {
                None
            };
            if operating_point_idc_0.is_none() {
                operating_point_idc_0 = Some(operating_point_idc);
            }
            if seq_level_idx_0.is_none() {
                seq_level_idx_0 = Some(level);
            }
            if seq_tier_0.is_none() {
                seq_tier_0 = tier;
            }
        }

        let frame_width_bits_minus_1 =
            bits.read_bits_with(4, ParseError::TruncatedSequenceHeader)? as u8;
        let frame_height_bits_minus_1 =
            bits.read_bits_with(4, ParseError::TruncatedSequenceHeader)? as u8;
        let max_frame_width = bits
            .read_bits_with(frame_width_bits_minus_1 + 1, ParseError::TruncatedSequenceHeader)?
            + 1;
        let max_frame_height = bits
            .read_bits_with(frame_height_bits_minus_1 + 1, ParseError::TruncatedSequenceHeader)?
            + 1;
        let frame_id_numbers_present_flag =
            bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
        (
            timing_info_present_flag,
            initial_display_delay_present_flag,
            frame_id_numbers_present_flag,
            operating_points_cnt_minus_1,
            operating_point_idc_0.unwrap_or(0),
            seq_level_idx_0.unwrap_or(0),
            seq_tier_0,
            max_frame_width,
            max_frame_height,
            frame_width_bits_minus_1 + 1,
            frame_height_bits_minus_1 + 1,
        )
    };

    let (bit_depth, monochrome, subsampling_x, subsampling_y, color_range, chroma_sample_position) =
        parse_color_config(&mut bits, profile, reduced_still_picture_header)?;

    Ok(SequenceHeader {
        profile,
        still_picture,
        single_picture_header_flag: reduced_still_picture_header,
        reduced_still_picture_header,
        timing_info_present_flag,
        initial_display_delay_present_flag,
        frame_id_numbers_present_flag,
        operating_points_cnt_minus_1,
        operating_point_idc_0,
        seq_level_idx_0,
        seq_tier_0,
        max_frame_width,
        max_frame_height,
        bit_depth,
        monochrome,
        subsampling_x,
        subsampling_y,
        color_range,
        chroma_sample_position,
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
        num_bits_width,
        num_bits_height,
        df_par_bits_minus2: 0,
    })
}

fn parse_sequence_header_av2(payload: &[u8]) -> Result<SequenceHeader, ParseError> {
    let mut bits = BitReader::new(payload);
    let _seq_header_id = bits.read_uvlc(ParseError::TruncatedSequenceHeader)?;
    let profile = bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as u8;
    let single_picture_header_flag = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let seq_level_idx_0 = bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as u8;
    let seq_tier_0 = if seq_level_idx_0 >= 8 && !single_picture_header_flag {
        Some(bits.read_bit_with(ParseError::TruncatedSequenceHeader)?)
    } else {
        None
    };

    let chroma_format_idc = bits.read_uvlc(ParseError::TruncatedSequenceHeader)?;
    let monochrome = chroma_format_idc == 1;
    let (subsampling_x, subsampling_y) = match chroma_format_idc {
        0 => (1, 1),
        1 => (1, 1),
        2 => (0, 0),
        3 => (1, 0),
        _ => return Err(ParseError::UnsupportedSequenceHeader),
    };

    let bit_depth = match bits.read_uvlc(ParseError::TruncatedSequenceHeader)? {
        0 => 10,
        1 => 8,
        2 => 12,
        _ => return Err(ParseError::UnsupportedSequenceHeader),
    };

    let still_picture = if single_picture_header_flag {
        true
    } else {
        let _seq_lcr_id = bits.read_bits_with(3, ParseError::TruncatedSequenceHeader)?;
        let still_picture = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
        let _max_tlayer_id = bits.read_bits_with(2, ParseError::TruncatedSequenceHeader)?;
        let max_mlayer_id = bits.read_bits_with(3, ParseError::TruncatedSequenceHeader)?;
        if max_mlayer_id > 0 {
            let mlayer_bits = (u32::BITS - max_mlayer_id.leading_zeros()) as u8;
            let _seq_max_mlayer_cnt_minus_1 =
                bits.read_bits_with(mlayer_bits, ParseError::TruncatedSequenceHeader)?;
        }
        let _monotonic_output_order_flag =
            bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
        still_picture
    };

    let frame_width_bits_minus_1 =
        bits.read_bits_with(4, ParseError::TruncatedSequenceHeader)? as u8;
    let frame_height_bits_minus_1 =
        bits.read_bits_with(4, ParseError::TruncatedSequenceHeader)? as u8;
    let max_frame_width = bits
        .read_bits_with(frame_width_bits_minus_1 + 1, ParseError::TruncatedSequenceHeader)?
        + 1;
    let max_frame_height = bits
        .read_bits_with(frame_height_bits_minus_1 + 1, ParseError::TruncatedSequenceHeader)?
        + 1;

    let conf_window_flag = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if conf_window_flag {
        let _left = bits.read_uvlc(ParseError::TruncatedSequenceHeader)?;
        let _right = bits.read_uvlc(ParseError::TruncatedSequenceHeader)?;
        let _top = bits.read_uvlc(ParseError::TruncatedSequenceHeader)?;
        let _bottom = bits.read_uvlc(ParseError::TruncatedSequenceHeader)?;
    }

    let toolset = if single_picture_header_flag {
        parse_single_picture_sequence_toolset(&mut bits, monochrome)?
    } else {
        SequenceToolset::defaults(monochrome, single_picture_header_flag)
    };

    Ok(SequenceHeader {
        profile,
        still_picture,
        single_picture_header_flag,
        reduced_still_picture_header: single_picture_header_flag,
        timing_info_present_flag: false,
        initial_display_delay_present_flag: false,
        frame_id_numbers_present_flag: false,
        operating_points_cnt_minus_1: 0,
        operating_point_idc_0: 0,
        seq_level_idx_0,
        seq_tier_0,
        max_frame_width,
        max_frame_height,
        bit_depth,
        monochrome,
        subsampling_x,
        subsampling_y,
        color_range: 0,
        chroma_sample_position: 0,
        force_screen_content_tools: toolset.force_screen_content_tools,
        force_integer_mv: toolset.force_integer_mv,
        enable_intra_edge_filter: toolset.enable_intra_edge_filter,
        enable_cdef: toolset.enable_cdef,
        enable_restoration: toolset.enable_restoration,
        separate_uv_delta_q: toolset.separate_uv_delta_q,
        equal_ac_dc_q: toolset.equal_ac_dc_q,
        base_y_dc_delta_q: toolset.base_y_dc_delta_q,
        y_dc_delta_q_enabled: toolset.y_dc_delta_q_enabled,
        base_uv_dc_delta_q: toolset.base_uv_dc_delta_q,
        uv_dc_delta_q_enabled: toolset.uv_dc_delta_q_enabled,
        base_uv_ac_delta_q: toolset.base_uv_ac_delta_q,
        uv_ac_delta_q_enabled: toolset.uv_ac_delta_q_enabled,
        reduced_tx_part_set: toolset.reduced_tx_part_set,
        film_grain_params_present: toolset.film_grain_params_present,
        num_bits_width: frame_width_bits_minus_1 + 1,
        num_bits_height: frame_height_bits_minus_1 + 1,
        df_par_bits_minus2: toolset.df_par_bits_minus2,
    })
}

fn parse_single_picture_sequence_toolset(
    bits: &mut BitReader<'_>,
    monochrome: bool,
) -> Result<SequenceToolset, ParseError> {
    let _sb_size = read_sb_size(bits)?;
    if !monochrome {
        let _enable_sdp = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    }
    let _enable_ext_partitions = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if _enable_ext_partitions {
        let _enable_uneven_4way = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    }
    if bits.read_bit_with(ParseError::TruncatedSequenceHeader)? {
        let _max_pb_aspect_ratio_log2_m1 =
            bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    }

    let _enable_ext_seg = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let seq_seg_info_present_flag = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if seq_seg_info_present_flag {
        return Err(ParseError::UnsupportedSequenceHeader);
    }

    let _enable_intra_dip = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let enable_intra_edge_filter = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let _enable_mrls = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let _enable_cfl_intra = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if !monochrome {
        let _cfl_ds_filter_index = bits.read_bits_with(2, ParseError::TruncatedSequenceHeader)?;
    }
    let _enable_mhccp = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let _enable_ibp = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;

    let _enable_refmvbank = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if bits.read_bit_with(ParseError::TruncatedSequenceHeader)? {
        let _enable_drl_reorder_disabled = true;
    } else {
        let _enable_drl_reorder_constraint =
            bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    }
    let _def_max_bvp_drl_bits =
        1 + read_quniform(bits, 3, ParseError::TruncatedSequenceHeader)?;
    let _allow_frame_max_bvp_drl_bits = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let _enable_bawp = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;

    let force_screen_content_tools = 2;
    let force_integer_mv = 2;

    let _enable_fsc = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let _enable_idtx_intra = true;
    let _enable_ist = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let _enable_inter_ist = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if !monochrome {
        let _enable_chroma_dctonly = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    }
    let reduced_tx_part_set = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if !monochrome {
        let _enable_cctx = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    }
    let enable_tcq = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if enable_tcq {
        let _tcq_mode = 1;
    }
    let _enable_parity_hiding = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let separate_uv_delta_q = if monochrome {
        false
    } else {
        bits.read_bit_with(ParseError::TruncatedSequenceHeader)?
    };
    let equal_ac_dc_q = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let (base_y_dc_delta_q, y_dc_delta_q_enabled) = if !equal_ac_dc_q {
        (
            bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as i16 - 12,
            bits.read_bit_with(ParseError::TruncatedSequenceHeader)?,
        )
    } else {
        (0, false)
    };
    let (base_uv_dc_delta_q, uv_dc_delta_q_enabled, base_uv_ac_delta_q, uv_ac_delta_q_enabled) =
        if monochrome {
            (0, false, 0, false)
        } else if equal_ac_dc_q {
            let base_uv_ac_delta_q =
                bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as i16 - 12;
            let uv_ac_delta_q_enabled = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
            (base_uv_ac_delta_q, false, base_uv_ac_delta_q, uv_ac_delta_q_enabled)
        } else {
            let base_uv_dc_delta_q =
                bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as i16 - 12;
            let uv_dc_delta_q_enabled = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
            let base_uv_ac_delta_q =
                bits.read_bits_with(5, ParseError::TruncatedSequenceHeader)? as i16 - 12;
            let uv_ac_delta_q_enabled = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
            (
                base_uv_dc_delta_q,
                uv_dc_delta_q_enabled,
                base_uv_ac_delta_q,
                uv_ac_delta_q_enabled,
            )
        };

    let _disable_loopfilters_across_tiles = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let enable_cdef = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let _enable_gdf = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let enable_restoration = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if enable_restoration {
        return Err(ParseError::UnsupportedSequenceHeader);
    }
    let _enable_ccso = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let _enable_cdef_on_skip_txfm = true;
    let df_par_bits_minus2 = bits.read_bits_with(2, ParseError::TruncatedSequenceHeader)? as u8;

    let seq_tile_info_present_flag = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if seq_tile_info_present_flag {
        return Err(ParseError::UnsupportedSequenceHeader);
    }

    let film_grain_params_present = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;

    Ok(SequenceToolset {
        force_screen_content_tools,
        force_integer_mv,
        enable_intra_edge_filter,
        enable_cdef,
        enable_restoration,
        separate_uv_delta_q,
        equal_ac_dc_q,
        base_y_dc_delta_q: clamp_i16_to_i8(base_y_dc_delta_q),
        y_dc_delta_q_enabled,
        base_uv_dc_delta_q: clamp_i16_to_i8(base_uv_dc_delta_q),
        uv_dc_delta_q_enabled,
        base_uv_ac_delta_q: clamp_i16_to_i8(base_uv_ac_delta_q),
        uv_ac_delta_q_enabled,
        reduced_tx_part_set,
        film_grain_params_present,
        df_par_bits_minus2,
    })
}

fn read_sb_size(bits: &mut BitReader<'_>) -> Result<u8, ParseError> {
    let mut index = 0u8;
    if !bits.read_bit_with(ParseError::TruncatedSequenceHeader)? {
        index += 1;
        if !bits.read_bit_with(ParseError::TruncatedSequenceHeader)? {
            index += 1;
        }
    }
    Ok(index)
}

fn read_quniform(
    bits: &mut BitReader<'_>,
    n: u32,
    err: ParseError,
) -> Result<u8, ParseError> {
    let l = 32 - (n.saturating_sub(1)).leading_zeros();
    let m = (1u32 << l) - n;
    let v = bits.read_bits_with((l - 1) as u8, err)?;
    if v < m {
        return Ok(v as u8);
    }
    let extra = u32::from(bits.read_bit_with(err)?);
    Ok(((v << 1) - m + extra) as u8)
}

fn parse_color_config(
    bits: &mut BitReader<'_>,
    profile: u8,
    reduced_still_picture_header: bool,
) -> Result<(u8, bool, u8, u8, u8, u8), ParseError> {
    let high_bitdepth = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    let bit_depth = if !high_bitdepth {
        8
    } else if profile == 2 {
        let twelve_bit = bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
        if twelve_bit { 12 } else { 10 }
    } else {
        10
    };

    let monochrome = if profile == 1 {
        false
    } else {
        bits.read_bit_with(ParseError::TruncatedSequenceHeader)?
    };

    if reduced_still_picture_header {
        let color_range = bits.read_bit_with(ParseError::TruncatedSequenceHeader)? as u8;
        return Ok((bit_depth, monochrome, 1, 1, color_range, 0));
    }

    let color_description_present_flag =
        bits.read_bit_with(ParseError::TruncatedSequenceHeader)?;
    if color_description_present_flag {
        let _color_primaries = bits.read_bits_with(8, ParseError::TruncatedSequenceHeader)?;
        let _transfer_characteristics =
            bits.read_bits_with(8, ParseError::TruncatedSequenceHeader)?;
        let _matrix_coefficients =
            bits.read_bits_with(8, ParseError::TruncatedSequenceHeader)?;
    }

    let color_range = bits.read_bit_with(ParseError::TruncatedSequenceHeader)? as u8;
    if monochrome {
        return Ok((bit_depth, true, 1, 1, color_range, 0));
    }

    let (subsampling_x, subsampling_y, chroma_sample_position) = match profile {
        0 => {
            let chroma_sample_position =
                bits.read_bits_with(2, ParseError::TruncatedSequenceHeader)? as u8;
            (1, 1, chroma_sample_position)
        }
        1 => (0, 0, 0),
        2 => {
            let subsampling_x = bits.read_bit_with(ParseError::TruncatedSequenceHeader)? as u8;
            let subsampling_y = bits.read_bit_with(ParseError::TruncatedSequenceHeader)? as u8;
            let chroma_sample_position = if subsampling_x == 1 && subsampling_y == 1 {
                bits.read_bits_with(2, ParseError::TruncatedSequenceHeader)? as u8
            } else {
                0
            };
            (subsampling_x, subsampling_y, chroma_sample_position)
        }
        _ => return Err(ParseError::UnsupportedSequenceHeader),
    };

    Ok((
        bit_depth,
        monochrome,
        subsampling_x,
        subsampling_y,
        color_range,
        chroma_sample_position,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_sequence_header(
        still_picture: bool,
        reduced_still_picture_header: bool,
        width: u32,
        height: u32,
    ) -> SequenceHeader {
        SequenceHeader {
            profile: 0,
            still_picture,
            single_picture_header_flag: reduced_still_picture_header,
            reduced_still_picture_header,
            timing_info_present_flag: false,
            initial_display_delay_present_flag: false,
            frame_id_numbers_present_flag: false,
            operating_points_cnt_minus_1: 0,
            operating_point_idc_0: 0,
            seq_level_idx_0: 0,
            seq_tier_0: None,
            max_frame_width: width,
            max_frame_height: height,
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
        }
    }

    struct BitWriter {
        bytes: Vec<u8>,
        bit_offset: usize,
    }

    impl BitWriter {
        fn new() -> Self {
            Self {
                bytes: Vec::new(),
                bit_offset: 0,
            }
        }

        fn push_bits(&mut self, value: u32, count: u8) {
            for shift in (0..count).rev() {
                let bit = ((value >> shift) & 1) as u8;
                let byte_index = self.bit_offset / 8;
                let bit_index = 7 - (self.bit_offset % 8);
                if self.bytes.len() <= byte_index {
                    self.bytes.push(0);
                }
                self.bytes[byte_index] |= bit << bit_index;
                self.bit_offset += 1;
            }
        }

        fn into_bytes(self) -> Vec<u8> {
            self.bytes
        }
    }

    #[test]
    fn parse_single_temporal_delimiter() {
        let packet = [0x12, 0x00];
        let obus = parse_obus(&packet).expect("valid OBU");
        assert_eq!(obus.len(), 1);
        assert_eq!(ObuType::from_raw(obus[0].header.obu_type), ObuType::TemporalDelimiter);
        assert!(obus[0].payload.is_empty());
    }

    #[test]
    fn parse_multiple_obus() {
        let packet = [0x12, 0x00, 0x2a, 0x01, 0xff];
        let obus = parse_obus(&packet).expect("valid OBU packet");
        assert_eq!(obus.len(), 2);
        assert_eq!(ObuType::from_raw(obus[1].header.obu_type), ObuType::Metadata);
        assert_eq!(obus[1].payload, &[0xff]);
    }

    #[test]
    fn reject_truncated_payload() {
        let packet = [0x32, 0x02, 0x00];
        let err = parse_obus(&packet).expect_err("truncated payload");
        assert_eq!(err, ParseError::TruncatedPayload);
    }

    #[test]
    fn parse_reduced_still_picture_sequence_header() {
        let mut bits = BitWriter::new();
        bits.push_bits(0, 5); // profile
        bits.push_bits(1, 1); // still_picture
        bits.push_bits(1, 1); // reduced_still_picture_header
        bits.push_bits(0, 5); // level
        bits.push_bits(7, 4); // frame_width_bits_minus_1 => 8 bits
        bits.push_bits(7, 4); // frame_height_bits_minus_1 => 8 bits
        bits.push_bits(63, 8); // width_minus_1 => 64
        bits.push_bits(47, 8); // height_minus_1 => 48
        bits.push_bits(0, 1); // high_bitdepth => 8-bit
        bits.push_bits(0, 1); // monochrome => false
        bits.push_bits(0, 1); // color_range => studio

        let header = parse_sequence_header(&bits.into_bytes()).expect("sequence header");
        assert_eq!(header.profile, 0);
        assert!(header.still_picture);
        assert!(header.reduced_still_picture_header);
        assert!(!header.timing_info_present_flag);
        assert!(!header.initial_display_delay_present_flag);
        assert!(!header.frame_id_numbers_present_flag);
        assert_eq!(header.operating_points_cnt_minus_1, 0);
        assert_eq!(header.operating_point_idc_0, 0);
        assert_eq!(header.seq_level_idx_0, 0);
        assert_eq!(header.seq_tier_0, None);
        assert_eq!(header.max_frame_width, 64);
        assert_eq!(header.max_frame_height, 48);
        assert_eq!(header.bit_depth, 8);
        assert!(!header.monochrome);
        assert_eq!(header.subsampling_x, 1);
        assert_eq!(header.subsampling_y, 1);
        assert_eq!(header.color_range, 0);
        assert_eq!(header.chroma_sample_position, 0);
    }

    #[test]
    fn parse_general_sequence_header_minimal_subset() {
        let mut bits = BitWriter::new();
        bits.push_bits(0, 5); // profile
        bits.push_bits(0, 1); // still_picture
        bits.push_bits(0, 1); // reduced_still_picture_header
        bits.push_bits(0, 1); // timing_info_present_flag
        bits.push_bits(0, 1); // initial_display_delay_present_flag
        bits.push_bits(0, 5); // operating_points_cnt_minus_1
        bits.push_bits(0, 12); // operating_point_idc
        bits.push_bits(0, 5); // seq_level_idx
        bits.push_bits(7, 4); // frame_width_bits_minus_1 => 8 bits
        bits.push_bits(7, 4); // frame_height_bits_minus_1 => 8 bits
        bits.push_bits(79, 8); // width_minus_1 => 80
        bits.push_bits(59, 8); // height_minus_1 => 60
        bits.push_bits(1, 1); // frame_id_numbers_present_flag
        bits.push_bits(0, 1); // high_bitdepth => 8-bit
        bits.push_bits(0, 1); // monochrome => false
        bits.push_bits(0, 1); // color_description_present_flag
        bits.push_bits(0, 1); // color_range => studio
        bits.push_bits(0, 2); // chroma_sample_position

        let header = parse_sequence_header(&bits.into_bytes()).expect("sequence header");
        assert_eq!(header.max_frame_width, 80);
        assert_eq!(header.max_frame_height, 60);
        assert!(!header.reduced_still_picture_header);
        assert!(!header.timing_info_present_flag);
        assert!(!header.initial_display_delay_present_flag);
        assert!(header.frame_id_numbers_present_flag);
        assert_eq!(header.operating_points_cnt_minus_1, 0);
        assert_eq!(header.operating_point_idc_0, 0);
        assert_eq!(header.seq_level_idx_0, 0);
        assert_eq!(header.seq_tier_0, None);
        assert_eq!(header.bit_depth, 8);
        assert!(!header.monochrome);
        assert_eq!(header.subsampling_x, 1);
        assert_eq!(header.subsampling_y, 1);
        assert_eq!(header.color_range, 0);
        assert_eq!(header.chroma_sample_position, 0);
    }

    #[test]
    fn parse_general_sequence_header_multiple_operating_points() {
        let mut bits = BitWriter::new();
        bits.push_bits(0, 5); // profile
        bits.push_bits(0, 1); // still_picture
        bits.push_bits(0, 1); // reduced_still_picture_header
        bits.push_bits(0, 1); // timing_info_present_flag
        bits.push_bits(0, 1); // initial_display_delay_present_flag
        bits.push_bits(1, 5); // operating_points_cnt_minus_1 => 2 operating points
        bits.push_bits(0x123, 12); // first operating_point_idc
        bits.push_bits(10, 5); // first seq_level_idx (>7)
        bits.push_bits(1, 1); // first seq_tier
        bits.push_bits(0x000, 12); // second operating_point_idc
        bits.push_bits(4, 5); // second seq_level_idx
        bits.push_bits(7, 4); // frame_width_bits_minus_1 => 8 bits
        bits.push_bits(7, 4); // frame_height_bits_minus_1 => 8 bits
        bits.push_bits(31, 8); // width_minus_1 => 32
        bits.push_bits(23, 8); // height_minus_1 => 24
        bits.push_bits(0, 1); // frame_id_numbers_present_flag
        bits.push_bits(0, 1); // high_bitdepth => 8-bit
        bits.push_bits(0, 1); // monochrome => false
        bits.push_bits(0, 1); // color_description_present_flag
        bits.push_bits(1, 1); // color_range => full
        bits.push_bits(2, 2); // chroma_sample_position

        let header = parse_sequence_header(&bits.into_bytes()).expect("sequence header");
        assert!(!header.frame_id_numbers_present_flag);
        assert_eq!(header.operating_points_cnt_minus_1, 1);
        assert_eq!(header.operating_point_idc_0, 0x123);
        assert_eq!(header.seq_level_idx_0, 10);
        assert_eq!(header.seq_tier_0, Some(true));
        assert_eq!(header.max_frame_width, 32);
        assert_eq!(header.max_frame_height, 24);
        assert_eq!(header.bit_depth, 8);
        assert!(!header.monochrome);
        assert_eq!(header.subsampling_x, 1);
        assert_eq!(header.subsampling_y, 1);
        assert_eq!(header.color_range, 1);
        assert_eq!(header.chroma_sample_position, 2);
    }

    #[test]
    fn parse_corpus_m0_sequence_header() {
        let payload = include_bytes!("../tests/corpora/m0/sh.bin");
        let header = parse_sequence_header(payload).expect("sequence header");
        assert!(header.reduced_still_picture_header);
        assert!(header.still_picture);
        assert_eq!(header.max_frame_width, 64);
        assert_eq!(header.max_frame_height, 64);
        assert_eq!(header.bit_depth, 8);
        assert!(!header.monochrome);
        assert_eq!(header.subsampling_x, 1);
        assert_eq!(header.subsampling_y, 1);
    }

    #[test]
    fn parse_corpus_m0_uncompressed_frame_header() {
        let sh = parse_sequence_header(include_bytes!("../tests/corpora/m0/sh.bin"))
            .expect("sequence header");
        let fh = parse_uncompressed_frame_header(&sh, include_bytes!("../tests/corpora/m0/fh.bin"))
            .expect("frame header");
        assert_eq!(fh.frame_type, FrameType::Key);
        assert!(fh.show_frame);
        assert_eq!(fh.quant.base_q_idx, 230);
        assert_eq!(fh.num_tile_cols, 1);
        assert_eq!(fh.num_tile_rows, 1);
        assert_eq!(fh.tx_mode, 0);
    }

    #[test]
    fn parse_corpus_m0_tile_group() {
        let sh = parse_sequence_header(include_bytes!("../tests/corpora/m0/sh.bin"))
            .expect("sequence header");
        let fh = parse_uncompressed_frame_header(&sh, include_bytes!("../tests/corpora/m0/fh.bin"))
            .expect("frame header");
        let tg = parse_tile_group(&sh, &fh, include_bytes!("../tests/corpora/m0/tg.bin"))
            .expect("tile group");
        assert_eq!(tg.tile_start, 0);
        assert_eq!(tg.tile_end, 0);
        assert!(!tg.data.is_empty());
    }

    #[test]
    fn classify_mixed_frame_packet() {
        let packet = [0x1a, 0x00, 0x22, 0x00];
        let obus = parse_obus(&packet).expect("valid OBU packet");
        assert_eq!(classify_frame_packet(&obus), Some(FramePacketKind::Mixed));
    }

    #[test]
    fn reduced_still_picture_implies_shown_key_frame() {
        let header = test_sequence_header(true, true, 64, 48);
        let frame = reduced_still_picture_frame_header(&header, FramePacketKind::Frame)
            .expect("frame semantics");
        assert_eq!(frame.frame_type, Some(FrameType::Key));
        assert!(frame.show_frame);
        assert!(!frame.show_existing_frame);
        assert_eq!(frame.existing_frame_idx, None);
        assert_eq!(frame.error_resilient_mode, Some(true));
        assert_eq!(frame.disable_cdf_update, None);
        assert_eq!(frame.primary_ref_frame, None);
        assert_eq!(frame.refresh_frame_flags, None);
        assert_eq!(frame.frame_size_override_flag, None);
    }

    #[test]
    fn parse_general_frame_header_prefix() {
        let header = test_sequence_header(false, false, 64, 48);
        // show_existing_frame=0, frame_type=01 (inter), show_frame=1,
        // error_resilient_mode=0, disable_cdf_update=1, primary_ref_frame=101,
        // refresh_frame_flags=1010_0101, frame_size_override_flag=1
        let parsed = parse_frame_header_info(&header, ObuType::FrameHeader, &[0x36, 0xd2, 0xc0])
            .expect("frame");
        let parsed = parsed.expect("frame header");
        assert_eq!(parsed.frame_type, Some(FrameType::Inter));
        assert!(parsed.show_frame);
        assert!(!parsed.show_existing_frame);
        assert_eq!(parsed.existing_frame_idx, None);
        assert_eq!(parsed.error_resilient_mode, Some(false));
        assert_eq!(parsed.disable_cdf_update, Some(true));
        assert_eq!(parsed.primary_ref_frame, Some(5));
        assert_eq!(parsed.refresh_frame_flags, Some(0xa5));
        assert_eq!(parsed.frame_size_override_flag, Some(true));
    }

    #[test]
    fn parse_general_show_existing_frame_prefix() {
        let header = test_sequence_header(false, false, 64, 48);
        // show_existing_frame=1, existing_frame_idx=010
        let parsed =
            parse_frame_header_info(&header, ObuType::FrameHeader, &[0b1010_0000]).expect("frame");
        let parsed = parsed.expect("frame header");
        assert_eq!(parsed.frame_type, None);
        assert!(parsed.show_frame);
        assert!(parsed.show_existing_frame);
        assert_eq!(parsed.existing_frame_idx, Some(2));
        assert_eq!(parsed.error_resilient_mode, None);
        assert_eq!(parsed.disable_cdf_update, None);
        assert_eq!(parsed.primary_ref_frame, None);
        assert_eq!(parsed.refresh_frame_flags, None);
        assert_eq!(parsed.frame_size_override_flag, None);
    }

    #[test]
    fn parse_reduced_still_uncompressed_frame_header() {
        let sh = test_sequence_header(true, true, 64, 48);
        let fh = parse_uncompressed_frame_header(&sh, &[24]).expect("frame header");
        assert_eq!(fh.frame_type, FrameType::Key);
        assert!(fh.show_frame);
        assert_eq!(fh.quant.base_q_idx, 24);
        assert_eq!(fh.num_tile_cols, 1);
        assert_eq!(fh.num_tile_rows, 1);
        assert_eq!(fh.tx_mode, 0);
    }

    #[test]
    fn split_frame_obu_payload_returns_tile_bytes_after_header() {
        let sh = test_sequence_header(true, true, 64, 48);
        let (fh, tile_payload) =
            split_frame_obu_payload(&sh, &[24, 0xaa, 0xbb]).expect("frame payload");
        assert_eq!(fh.quant.base_q_idx, 24);
        assert_eq!(tile_payload, &[0xaa, 0xbb]);
    }

    #[test]
    fn split_general_frame_obu_payload_uses_overridden_frame_size() {
        let sh = test_sequence_header(false, false, 64, 48);
        let mut bits = BitWriter::new();
        bits.push_bits(0, 1); // show_existing_frame
        bits.push_bits(0, 2); // frame_type = key
        bits.push_bits(1, 1); // show_frame
        bits.push_bits(1, 1); // error_resilient_mode
        bits.push_bits(0, 1); // disable_cdf_update
        bits.push_bits(0, 3); // primary_ref_frame
        bits.push_bits(0xff, 8); // refresh_frame_flags
        bits.push_bits(1, 1); // frame_size_override_flag
        bits.push_bits(31, 8); // width_minus_1 => 32
        bits.push_bits(15, 8); // height_minus_1 => 16
        bits.push_bits(24, 8); // base_q_idx
        bits.push_bits(0, 1); // segmentation.enabled
        bits.push_bits(0, 1); // using_qmatrix
        bits.push_bits(0, 1); // delta_q_present
        bits.push_bits(0, 1); // apply_deblocking_filter[0]
        bits.push_bits(0, 1); // apply_deblocking_filter[1]
        bits.push_bits(0, 1); // tx_mode = largest
        bits.push_bits(0, 2); // reduced_tx_set_used
        let mut payload = bits.into_bytes();
        payload.push(0xaa);

        let (fh, tile_payload) = split_frame_obu_payload(&sh, &payload).expect("frame payload");
        assert_eq!(fh.frame_width, 32);
        assert_eq!(fh.frame_height, 16);
        assert_eq!(fh.render_size.width, 32);
        assert_eq!(fh.render_size.height, 16);
        assert_eq!(fh.quant.base_q_idx, 24);
        assert_eq!(tile_payload, &[0xaa]);
    }

    #[test]
    fn split_lossless_general_frame_obu_payload_consumes_tail_header_bits() {
        let sh = test_sequence_header(false, false, 64, 48);
        let mut bits = BitWriter::new();
        bits.push_bits(0, 1); // show_existing_frame
        bits.push_bits(0, 2); // frame_type = key
        bits.push_bits(1, 1); // show_frame
        bits.push_bits(1, 1); // error_resilient_mode
        bits.push_bits(0, 1); // disable_cdf_update
        bits.push_bits(0, 3); // primary_ref_frame
        bits.push_bits(0xff, 8); // refresh_frame_flags
        bits.push_bits(0, 1); // frame_size_override_flag
        bits.push_bits(0, 8); // base_q_idx => lossless
        bits.push_bits(0, 1); // segmentation.enabled
        bits.push_bits(0, 1); // using_qmatrix
        bits.push_bits(0b10, 2); // reduced_tx_set_used
        let mut payload = bits.into_bytes();
        payload.push(0xaa);

        let (fh, tile_payload) = split_frame_obu_payload(&sh, &payload).expect("frame payload");
        assert_eq!(fh.quant.base_q_idx, 0);
        assert!(!fh.segmentation.enabled);
        assert!(!fh.quant.using_qmatrix);
        assert_eq!(fh.tx_mode, 0);
        assert!(fh.reduced_tx_set);
        assert_eq!(tile_payload, &[0xaa]);
    }

    #[test]
    fn split_non_lossless_general_frame_obu_payload_consumes_supported_tail_bits() {
        let sh = test_sequence_header(false, false, 64, 48);
        let mut bits = BitWriter::new();
        bits.push_bits(0, 1); // show_existing_frame
        bits.push_bits(0, 2); // frame_type = key
        bits.push_bits(1, 1); // show_frame
        bits.push_bits(1, 1); // error_resilient_mode
        bits.push_bits(0, 1); // disable_cdf_update
        bits.push_bits(0, 3); // primary_ref_frame
        bits.push_bits(0xff, 8); // refresh_frame_flags
        bits.push_bits(0, 1); // frame_size_override_flag
        bits.push_bits(24, 8); // base_q_idx => non-lossless
        bits.push_bits(0, 1); // segmentation.enabled
        bits.push_bits(0, 1); // using_qmatrix
        bits.push_bits(0, 1); // delta_q_present
        bits.push_bits(0, 1); // apply_deblocking_filter[0]
        bits.push_bits(0, 1); // apply_deblocking_filter[1]
        bits.push_bits(1, 1); // tx_mode = select
        bits.push_bits(0b01, 2); // reduced_tx_set_used
        let mut payload = bits.into_bytes();
        payload.push(0xaa);

        let (fh, tile_payload) = split_frame_obu_payload(&sh, &payload).expect("frame payload");
        assert_eq!(fh.quant.base_q_idx, 24);
        assert!(!fh.segmentation.enabled);
        assert!(!fh.quant.using_qmatrix);
        assert!(!fh.delta_q.present);
        assert_eq!(fh.tx_mode, 1);
        assert!(fh.reduced_tx_set);
        assert_eq!(tile_payload, &[0xaa]);
    }

    #[test]
    fn split_non_lossless_general_frame_obu_payload_consumes_deblocking_bits() {
        let sh = test_sequence_header(false, false, 64, 48);
        let mut bits = BitWriter::new();
        bits.push_bits(0, 1); // show_existing_frame
        bits.push_bits(0, 2); // frame_type = key
        bits.push_bits(1, 1); // show_frame
        bits.push_bits(1, 1); // error_resilient_mode
        bits.push_bits(0, 1); // disable_cdf_update
        bits.push_bits(0, 3); // primary_ref_frame
        bits.push_bits(0xff, 8); // refresh_frame_flags
        bits.push_bits(0, 1); // frame_size_override_flag
        bits.push_bits(24, 8); // base_q_idx => non-lossless
        bits.push_bits(0, 1); // segmentation.enabled
        bits.push_bits(0, 1); // using_qmatrix
        bits.push_bits(0, 1); // delta_q_present
        bits.push_bits(1, 1); // apply_deblocking_filter[0]
        bits.push_bits(0, 1); // apply_deblocking_filter[1]
        bits.push_bits(1, 1); // apply_deblocking_filter_u
        bits.push_bits(0, 1); // apply_deblocking_filter_v
        bits.push_bits(1, 1); // luma_delta_q present
        bits.push_bits(0b11, 2); // delta_q_luma[0] => +1
        bits.push_bits(0, 1); // u_delta_q absent => 0
        bits.push_bits(0, 1); // tx_mode = largest
        bits.push_bits(0, 2); // reduced_tx_set_used
        let mut payload = bits.into_bytes();
        payload.push(0xaa);

        let (fh, tile_payload) = split_frame_obu_payload(&sh, &payload).expect("frame payload");
        assert_eq!(fh.loop_filter.level, [1, 0, 1, 0]);
        assert!(fh.loop_filter.delta_enabled);
        assert!(fh.loop_filter.delta_update);
        assert_eq!(fh.tx_mode, 0);
        assert!(!fh.reduced_tx_set);
        assert_eq!(tile_payload, &[0xaa]);
    }

    #[test]
    fn split_non_lossless_general_frame_obu_payload_consumes_delta_q_bits() {
        let sh = test_sequence_header(false, false, 64, 48);
        let mut bits = BitWriter::new();
        bits.push_bits(0, 1); // show_existing_frame
        bits.push_bits(0, 2); // frame_type = key
        bits.push_bits(1, 1); // show_frame
        bits.push_bits(1, 1); // error_resilient_mode
        bits.push_bits(0, 1); // disable_cdf_update
        bits.push_bits(0, 3); // primary_ref_frame
        bits.push_bits(0xff, 8); // refresh_frame_flags
        bits.push_bits(0, 1); // frame_size_override_flag
        bits.push_bits(24, 8); // base_q_idx => non-lossless
        bits.push_bits(0, 1); // segmentation.enabled
        bits.push_bits(0, 1); // using_qmatrix
        bits.push_bits(1, 1); // delta_q_present
        bits.push_bits(0b10, 2); // delta_q_res => 4
        bits.push_bits(0, 1); // apply_deblocking_filter[0]
        bits.push_bits(0, 1); // apply_deblocking_filter[1]
        bits.push_bits(0, 1); // tx_mode = largest
        bits.push_bits(0, 2); // reduced_tx_set_used
        let mut payload = bits.into_bytes();
        payload.push(0xaa);

        let (fh, tile_payload) = split_frame_obu_payload(&sh, &payload).expect("frame payload");
        assert!(fh.delta_q.present);
        assert_eq!(fh.delta_q.scale, 4);
        assert_eq!(fh.tx_mode, 0);
        assert!(!fh.reduced_tx_set);
        assert_eq!(tile_payload, &[0xaa]);
    }

    #[test]
    fn split_non_lossless_general_frame_obu_payload_consumes_qmatrix_bits() {
        let sh = test_sequence_header(false, false, 64, 48);
        let mut bits = BitWriter::new();
        bits.push_bits(0, 1); // show_existing_frame
        bits.push_bits(0, 2); // frame_type = key
        bits.push_bits(1, 1); // show_frame
        bits.push_bits(1, 1); // error_resilient_mode
        bits.push_bits(0, 1); // disable_cdf_update
        bits.push_bits(0, 3); // primary_ref_frame
        bits.push_bits(0xff, 8); // refresh_frame_flags
        bits.push_bits(0, 1); // frame_size_override_flag
        bits.push_bits(24, 8); // base_q_idx => non-lossless
        bits.push_bits(0, 1); // segmentation.enabled
        bits.push_bits(1, 1); // using_qmatrix
        bits.push_bits(0b0110, 4); // qm_y = 6
        bits.push_bits(1, 1); // qm_uv_same_as_y
        bits.push_bits(0, 1); // delta_q_present
        bits.push_bits(0, 1); // apply_deblocking_filter[0]
        bits.push_bits(0, 1); // apply_deblocking_filter[1]
        bits.push_bits(0, 1); // tx_mode = largest
        bits.push_bits(0, 2); // reduced_tx_set_used
        let mut payload = bits.into_bytes();
        payload.push(0xaa);

        let (fh, tile_payload) = split_frame_obu_payload(&sh, &payload).expect("frame payload");
        assert!(fh.quant.using_qmatrix);
        assert_eq!(fh.quant.qm_y, 6);
        assert_eq!(fh.quant.qm_u, 6);
        assert_eq!(fh.quant.qm_v, 6);
        assert_eq!(tile_payload, &[0xaa]);
    }

    #[test]
    fn split_non_lossless_general_frame_obu_payload_consumes_qmatrix_uv_split_bits() {
        let sh = test_sequence_header(false, false, 64, 48);
        let mut bits = BitWriter::new();
        bits.push_bits(0, 1); // show_existing_frame
        bits.push_bits(0, 2); // frame_type = key
        bits.push_bits(1, 1); // show_frame
        bits.push_bits(1, 1); // error_resilient_mode
        bits.push_bits(0, 1); // disable_cdf_update
        bits.push_bits(0, 3); // primary_ref_frame
        bits.push_bits(0xff, 8); // refresh_frame_flags
        bits.push_bits(0, 1); // frame_size_override_flag
        bits.push_bits(24, 8); // base_q_idx => non-lossless
        bits.push_bits(0, 1); // segmentation.enabled
        bits.push_bits(1, 1); // using_qmatrix
        bits.push_bits(0b0011, 4); // qm_y = 3
        bits.push_bits(0, 1); // qm_uv_same_as_y = false
        bits.push_bits(0b1001, 4); // qm_u = 9
        bits.push_bits(0, 1); // delta_q_present
        bits.push_bits(0, 1); // apply_deblocking_filter[0]
        bits.push_bits(0, 1); // apply_deblocking_filter[1]
        bits.push_bits(0, 1); // tx_mode = largest
        bits.push_bits(0, 2); // reduced_tx_set_used
        let mut payload = bits.into_bytes();
        payload.push(0xaa);

        let (fh, tile_payload) = split_frame_obu_payload(&sh, &payload).expect("frame payload");
        assert!(fh.quant.using_qmatrix);
        assert_eq!(fh.quant.qm_y, 3);
        assert_eq!(fh.quant.qm_u, 9);
        assert_eq!(fh.quant.qm_v, 9);
        assert_eq!(tile_payload, &[0xaa]);
    }

    #[test]
    fn parse_single_tile_group_payload() {
        let sh = test_sequence_header(true, true, 64, 64);
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
        let tg = parse_tile_group(&sh, &fh, &[0x00]).expect("tile group");
        assert_eq!(tg.tile_start, 0);
        assert_eq!(tg.tile_end, 0);
        assert_eq!(tg.data, &[0x00]);
    }
}
