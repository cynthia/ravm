use crate::decoder::{DecoderError, FrameBufferManager, StreamInfo};
use crate::bitstream::{
    classify_frame_packet, parse_frame_header_info, parse_obus_auto, parse_sequence_header,
    parse_tile_group, split_frame_obu_payload, FrameHeaderInfo, FramePacketKind, ObuType,
    ParseError, SequenceHeader,
};
use crate::decoder::{DecodeEvent, DecodeProgress};
use crate::decoder::core::{decode_frame, CoreDecodeError};
use crate::decoder::frame_buffer::FrameBuffer as ReconstructedFrame;
use crate::sys::{avm_codec_frame_buffer_t, avm_codec_iter_t, avm_image_t};
use std::fmt;
use std::mem::MaybeUninit;
use std::os::raw::{c_int, c_void};
use std::ptr::NonNull;

/// Pure-Rust AV2 decoder backend.
///
/// M0 implements the walking-skeleton subset only: reduced-still-picture,
/// 8-bit 4:2:0 keyframes with a single tile, DC intra prediction, and the
/// minimal entropy/transform path needed by the checked-in corpus.
pub(crate) struct RustDecoder {
    saw_sequence_header: bool,
    packets_parsed: usize,
    obus_parsed: usize,
    frame_packets_seen: usize,
    last_frame_packet_kind: Option<FramePacketKind>,
    last_frame_header: Option<FrameHeaderInfo>,
    last_event: Option<DecodeEvent>,
    recent_events: [Option<DecodeEvent>; 4],
    sequence_header: Option<SequenceHeader>,
    stream_info: Option<StreamInfo>,
    pending_frame: Option<RustFrame>,
    frame_iter_active: bool,
}

impl fmt::Debug for RustDecoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RustDecoder")
    }
}

impl RustDecoder {
    pub(crate) fn new(_threads: Option<u32>) -> Result<Self, DecoderError> {
        Ok(Self {
            saw_sequence_header: false,
            packets_parsed: 0,
            obus_parsed: 0,
            frame_packets_seen: 0,
            last_frame_packet_kind: None,
            last_frame_header: None,
            last_event: None,
            recent_events: [None; 4],
            sequence_header: None,
            stream_info: None,
            pending_frame: None,
            frame_iter_active: false,
        })
    }

    fn push_event(&mut self, event: DecodeEvent) {
        self.recent_events.rotate_left(1);
        self.recent_events[self.recent_events.len() - 1] = Some(event);
        self.last_event = Some(event);
    }

    pub(crate) fn decode(&mut self, data: &[u8]) -> Result<(), DecoderError> {
        if data.is_empty() {
            return Ok(());
        }

        let obus = parse_obus_auto(data).map_err(map_parse_error)?;
        self.packets_parsed += 1;
        self.obus_parsed += obus.len();
        self.last_frame_packet_kind = classify_frame_packet(&obus);
        self.last_frame_header = None;
        self.last_event = None;
        if self.last_frame_packet_kind.is_some() {
            self.frame_packets_seen += 1;
        }

        let mut pending_frame_payload = None;
        let mut saw_frame_data = false;
        for obu in obus {
            let obu_type = ObuType::from_raw(obu.header.obu_type);
            if obu_type == ObuType::SequenceHeader {
                let header = parse_sequence_header(obu.payload).map_err(map_parse_error)?;
                self.saw_sequence_header = true;
                self.sequence_header = Some(header);
                self.stream_info = Some(StreamInfo {
                    width: header.max_frame_width,
                    height: header.max_frame_height,
                    is_kf: false,
                    number_tlayers: 1,
                    number_mlayers: 1,
                    number_xlayers: 1,
                });
                self.push_event(DecodeEvent::SequenceHeader(header));
            }
            if let Some(sequence_header) = self.sequence_header {
                if let Some(frame_header) =
                    parse_frame_header_info(&sequence_header, obu_type, obu.payload)
                        .map_err(map_parse_error)?
                {
                    self.last_frame_header = Some(frame_header);
                    self.push_event(DecodeEvent::FrameHeader(frame_header));
                }
            }
            if obu_type.is_frame_data() {
                saw_frame_data = true;
            }
            if matches!(obu_type, ObuType::Frame) {
                pending_frame_payload = Some(obu.payload);
            }
        }

        if let Some(frame_payload) = pending_frame_payload {
            if let Some(sequence_header) = self.sequence_header {
                if frame_payload.is_empty() {
                    return Err(DecoderError::Unimplemented(
                        "Rust backend parsed frame data without a complete frame payload",
                    ));
                }
                let (uncompressed, tile_payload) =
                    split_frame_obu_payload(&sequence_header, frame_payload)
                        .map_err(map_parse_error)?;
                let tile_group =
                    parse_tile_group(&sequence_header, &uncompressed, tile_payload)
                        .map_err(map_parse_error)?;
                let frame =
                    decode_frame(sequence_header, uncompressed, &tile_group)
                        .map_err(map_core_decode_error)?;
                self.pending_frame = Some(RustFrame::from_frame_buffer(frame, sequence_header));
                self.frame_iter_active = false;
                return Ok(());
            }
            Err(DecoderError::Unimplemented(
                "Rust backend parsed frame data without enough header state",
            ))
        } else if saw_frame_data {
            Err(DecoderError::Unimplemented(
                "Rust backend parsed OBU packets but full frame decoding is not implemented yet",
            ))
        } else {
            Ok(())
        }
    }

    pub(crate) fn flush(&mut self) -> Result<(), DecoderError> {
        Ok(())
    }

    pub(crate) fn get_stream_info(&mut self) -> Result<StreamInfo, DecoderError> {
        if let Some(info) = self.stream_info {
            Ok(info)
        } else if self.saw_sequence_header {
            Err(DecoderError::Unimplemented(
                "Rust backend observed a sequence header but has no stream info",
            ))
        } else {
            Err(DecoderError::Unimplemented(
                "Rust backend has not observed stream metadata yet",
            ))
        }
    }

    pub(crate) unsafe fn set_frame_buffer_functions(
        &mut self,
        _get_fb: unsafe extern "C" fn(
            priv_: *mut c_void,
            min_size: usize,
            fb: *mut avm_codec_frame_buffer_t,
        ) -> c_int,
        _release_fb: unsafe extern "C" fn(
            priv_: *mut c_void,
            fb: *mut avm_codec_frame_buffer_t,
        ) -> c_int,
        _priv_: *mut c_void,
    ) -> Result<(), DecoderError> {
        Err(DecoderError::BackendUnavailable(crate::backend::BackendKind::Rust))
    }

    pub(crate) fn set_frame_buffer_manager<M: FrameBufferManager + 'static>(
        &mut self,
        _manager: M,
    ) -> Result<(), DecoderError> {
        Err(DecoderError::BackendUnavailable(crate::backend::BackendKind::Rust))
    }

    pub(crate) fn get_frame(
        &mut self,
        iter: &mut avm_codec_iter_t,
    ) -> Option<NonNull<avm_image_t>> {
        if self.frame_iter_active {
            return None;
        }
        let frame = self.pending_frame.as_mut()?;
        self.frame_iter_active = true;
        *iter = NonNull::<c_void>::dangling().as_ptr();
        Some(NonNull::from(&mut frame.image))
    }

    pub(crate) fn progress(&self) -> DecodeProgress {
        DecodeProgress {
            backend: crate::backend::BackendKind::Rust,
            packets_parsed: Some(self.packets_parsed),
            obus_parsed: Some(self.obus_parsed),
            frame_packets_seen: Some(self.frame_packets_seen),
            sequence_header: self.sequence_header,
            stream_info: self.stream_info,
            last_frame_packet_kind: self.last_frame_packet_kind,
            last_frame_header: self.last_frame_header,
            last_event: self.last_event,
            recent_events: self.recent_events,
        }
    }
}

struct RustFrame {
    image: avm_image_t,
    _luma: Vec<u8>,
    _chroma_u: Vec<u8>,
    _chroma_v: Vec<u8>,
}

impl RustFrame {
    fn from_frame_buffer(frame: ReconstructedFrame<u8>, sequence_header: SequenceHeader) -> Self {
        let subsampling = frame.subsampling();
        let mut luma = frame.luma().data().to_vec();
        let mut chroma_u = frame.chroma_u().data().to_vec();
        let mut chroma_v = frame.chroma_v().data().to_vec();

        let mut image = zeroed_image();
        image.fmt = crate::sys::avm_img_fmt_AVM_IMG_FMT_I420;
        image.monochrome = 0;
        image.csp = match sequence_header.chroma_sample_position {
            0 => crate::sys::avm_chroma_sample_position_AVM_CSP_LEFT,
            1 => crate::sys::avm_chroma_sample_position_AVM_CSP_CENTER,
            2 => crate::sys::avm_chroma_sample_position_AVM_CSP_TOPLEFT,
            3 => crate::sys::avm_chroma_sample_position_AVM_CSP_TOP,
            4 => crate::sys::avm_chroma_sample_position_AVM_CSP_BOTTOMLEFT,
            5 => crate::sys::avm_chroma_sample_position_AVM_CSP_BOTTOM,
            _ => crate::sys::avm_chroma_sample_position_AVM_CSP_UNSPECIFIED,
        };
        image.range = if sequence_header.color_range == 0 {
            crate::sys::avm_color_range_AVM_CR_STUDIO_RANGE
        } else {
            crate::sys::avm_color_range_AVM_CR_FULL_RANGE
        };
        image.w = frame.luma().width as u32;
        image.h = frame.luma().height as u32;
        image.bit_depth = 8;
        image.max_width = frame.luma().width as i32;
        image.max_height = frame.luma().height as i32;
        image.crop_width = frame.luma().width as i32;
        image.crop_height = frame.luma().height as i32;
        image.d_w = frame.luma().width as u32;
        image.d_h = frame.luma().height as u32;
        image.r_w = frame.luma().width as u32;
        image.r_h = frame.luma().height as u32;
        let (x_shift, y_shift) = match subsampling {
            crate::format::Subsampling::Yuv420 => (1, 1),
            crate::format::Subsampling::Yuv422 => (1, 0),
            crate::format::Subsampling::Yuv444 => (0, 0),
        };
        image.x_chroma_shift = x_shift;
        image.y_chroma_shift = y_shift;
        image.planes = [
            luma.as_mut_ptr(),
            chroma_u.as_mut_ptr(),
            chroma_v.as_mut_ptr(),
        ];
        image.stride = [
            frame.luma().stride as c_int,
            frame.chroma_u().stride as c_int,
            frame.chroma_v().stride as c_int,
        ];
        image.sz = luma.len() + chroma_u.len() + chroma_v.len();
        image.img_data = luma.as_mut_ptr();

        Self {
            image,
            _luma: luma,
            _chroma_u: chroma_u,
            _chroma_v: chroma_v,
        }
    }
}

fn zeroed_image() -> avm_image_t {
    unsafe { MaybeUninit::<avm_image_t>::zeroed().assume_init() }
}

fn map_parse_error(err: ParseError) -> DecoderError {
    match err {
        ParseError::TruncatedHeader => DecoderError::Parse("truncated OBU header"),
        ParseError::InvalidHeader => DecoderError::Parse("invalid OBU header"),
        ParseError::TruncatedExtension => DecoderError::Parse("truncated OBU extension header"),
        ParseError::InvalidExtension => DecoderError::Parse("invalid OBU extension header"),
        ParseError::TruncatedSize => DecoderError::Parse("truncated OBU size field"),
        ParseError::SizeOverflow => DecoderError::Parse("OBU size overflow"),
        ParseError::TruncatedPayload => DecoderError::Parse("OBU payload exceeds packet length"),
        ParseError::TruncatedSequenceHeader => DecoderError::Parse("truncated sequence header"),
        ParseError::TruncatedFrameHeader => DecoderError::Parse("truncated frame header"),
        ParseError::TruncatedTileGroup => DecoderError::Parse("truncated tile group"),
        ParseError::UnsupportedSequenceHeader => {
            DecoderError::Unimplemented("unsupported sequence header form")
        }
        ParseError::UnsupportedFrameHeader => {
            DecoderError::Unimplemented("unsupported frame header form")
        }
    }
}

fn map_core_decode_error(err: CoreDecodeError) -> DecoderError {
    match err {
        CoreDecodeError::Unsupported(message) => DecoderError::Unimplemented(message),
        CoreDecodeError::UnexpectedMode => {
            DecoderError::Unimplemented("walking skeleton only supports DC intra mode")
        }
        CoreDecodeError::EntropyError => {
            DecoderError::Unimplemented("walking skeleton entropy path is incomplete")
        }
    }
}
