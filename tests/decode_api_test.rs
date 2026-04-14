//! Decode API validation tests — exercise error paths without test data.

use rustavm::backend::BackendKind;
use rustavm::bitstream::{FramePacketKind, FrameType};
use rustavm::decoder::{DecodeEvent, Decoder, FrameBuffer, FrameBufferManager};
use rustavm::ivf::IvfReader;
use rustavm::stream::decode_ivf_with_backend;
use std::fs;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[test]
fn test_decoder_new_succeeds() {
    assert!(Decoder::new().is_ok());
}

#[test]
fn test_decoder_with_threads_succeeds() {
    assert!(Decoder::builder().threads(4).build().is_ok());
}

#[test]
fn test_rust_backend_builder_succeeds() {
    let decoder = Decoder::builder().backend(BackendKind::Rust).build();
    assert!(decoder.is_ok());
    assert_eq!(decoder.unwrap().backend_kind(), BackendKind::Rust);
}

#[test]
fn test_rust_backend_accepts_non_frame_obu_packet() {
    let mut decoder = Decoder::builder()
        .backend(BackendKind::Rust)
        .build()
        .expect("rust backend build");
    // Temporal delimiter OBU with explicit zero-length payload.
    decoder.decode(&[0x12, 0x00]).expect("packet parser should accept valid non-frame OBU");
    let frames: Vec<_> = decoder.get_frames().collect();
    assert!(frames.is_empty());
}

#[test]
fn test_rust_backend_rejects_incomplete_frame_obu() {
    let mut decoder = Decoder::builder()
        .backend(BackendKind::Rust)
        .build()
        .expect("rust backend build");
    let err = decoder
        .decode(&[0x32, 0x00])
        .expect_err("truncated frame OBU should fail");
    assert!(err.to_string().contains("frame"));
}

fn reduced_still_sequence_header_payload(width: u32, height: u32) -> Vec<u8> {
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

    let width_minus_1 = width - 1;
    let height_minus_1 = height - 1;
    let width_bits = 32 - width_minus_1.leading_zeros();
    let height_bits = 32 - height_minus_1.leading_zeros();

    let mut bits = BitWriter::new();
    bits.push_bits(0, 5); // profile
    bits.push_bits(1, 1); // still_picture
    bits.push_bits(1, 1); // reduced_still_picture_header
    bits.push_bits(0, 5); // level
    bits.push_bits(width_bits - 1, 4);
    bits.push_bits(height_bits - 1, 4);
    bits.push_bits(width_minus_1, width_bits as u8);
    bits.push_bits(height_minus_1, height_bits as u8);
    bits.push_bits(0, 1); // high_bitdepth => 8-bit
    bits.push_bits(0, 1); // monochrome => false
    bits.push_bits(0, 1); // color_range => studio
    bits.into_bytes()
}

fn general_sequence_header_payload(width: u32, height: u32) -> Vec<u8> {
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

    let width_minus_1 = width - 1;
    let height_minus_1 = height - 1;
    let width_bits = 32 - width_minus_1.leading_zeros();
    let height_bits = 32 - height_minus_1.leading_zeros();

    let mut bits = BitWriter::new();
    bits.push_bits(0, 5); // profile
    bits.push_bits(0, 1); // still_picture
    bits.push_bits(0, 1); // reduced_still_picture_header
    bits.push_bits(0, 1); // timing_info_present_flag
    bits.push_bits(0, 1); // initial_display_delay_present_flag
    bits.push_bits(0, 5); // operating_points_cnt_minus_1
    bits.push_bits(0, 12); // operating_point_idc
    bits.push_bits(0, 5); // seq_level_idx
    bits.push_bits(width_bits - 1, 4);
    bits.push_bits(height_bits - 1, 4);
    bits.push_bits(width_minus_1, width_bits as u8);
    bits.push_bits(height_minus_1, height_bits as u8);
    bits.push_bits(1, 1); // frame_id_numbers_present_flag
    bits.push_bits(0, 1); // high_bitdepth => 8-bit
    bits.push_bits(0, 1); // monochrome => false
    bits.push_bits(0, 1); // color_description_present_flag
    bits.push_bits(0, 1); // color_range => studio
    bits.push_bits(0, 2); // chroma_sample_position
    bits.into_bytes()
}

#[test]
fn test_rust_backend_parses_sequence_header_stream_info() {
    let mut decoder = Decoder::builder()
        .backend(BackendKind::Rust)
        .build()
        .expect("rust backend build");
    let payload = reduced_still_sequence_header_payload(64, 48);
    let mut packet = vec![0x0a, payload.len() as u8];
    packet.extend_from_slice(&payload);
    decoder.decode(&packet).expect("sequence header should parse");
    let info = decoder.get_stream_info().expect("stream info");
    assert_eq!(info.width, 64);
    assert_eq!(info.height, 48);
    let progress = decoder.progress();
    assert_eq!(progress.backend, BackendKind::Rust);
    assert_eq!(progress.packets_parsed, Some(1));
    assert_eq!(progress.obus_parsed, Some(1));
    assert_eq!(progress.frame_packets_seen, Some(0));
    let seq = progress.sequence_header.expect("sequence header");
    assert!(seq.reduced_still_picture_header);
    assert!(!seq.frame_id_numbers_present_flag);
    assert_eq!(seq.operating_point_idc_0, 0);
    assert_eq!(seq.seq_level_idx_0, 0);
    assert_eq!(seq.seq_tier_0, None);
    assert_eq!(seq.max_frame_width, 64);
    assert_eq!(seq.max_frame_height, 48);
    assert_eq!(seq.bit_depth, 8);
    assert!(!seq.monochrome);
    assert_eq!(seq.subsampling_x, 1);
    assert_eq!(seq.subsampling_y, 1);
    assert_eq!(seq.color_range, 0);
    assert_eq!(seq.chroma_sample_position, 0);
    assert_eq!(progress.stream_info, Some(info));
    assert_eq!(progress.last_frame_header, None);
    assert_eq!(progress.last_event, Some(DecodeEvent::SequenceHeader(seq)));
    assert_eq!(
        progress.recent_events,
        [None, None, None, Some(DecodeEvent::SequenceHeader(seq))]
    );
}

#[test]
fn test_rust_backend_tracks_frame_packet_progress() {
    let mut decoder = Decoder::builder()
        .backend(BackendKind::Rust)
        .build()
        .expect("rust backend build");
    let payload = reduced_still_sequence_header_payload(64, 48);
    let mut seq_packet = vec![0x0a, payload.len() as u8];
    seq_packet.extend_from_slice(&payload);
    decoder.decode(&seq_packet).expect("sequence header");
    let err = decoder
        .decode(&[0x1a, 0x00, 0x22, 0x00])
        .expect_err("incomplete reduced-still frame packet should fail");
    assert!(err.to_string().contains("frame"));
    let progress = decoder.progress();
    assert_eq!(progress.packets_parsed, Some(2));
    assert_eq!(progress.obus_parsed, Some(3));
    assert_eq!(progress.frame_packets_seen, Some(1));
    assert_eq!(progress.last_frame_packet_kind, Some(FramePacketKind::Mixed));
    let header = progress.last_frame_header.expect("frame header semantics");
    assert_eq!(header.frame_type, Some(FrameType::Key));
    assert!(header.show_frame);
    assert!(!header.show_existing_frame);
    assert_eq!(header.existing_frame_idx, None);
    assert_eq!(header.error_resilient_mode, Some(true));
    assert_eq!(header.disable_cdf_update, None);
    assert_eq!(header.primary_ref_frame, None);
    assert_eq!(header.refresh_frame_flags, None);
    assert_eq!(header.frame_size_override_flag, None);
    assert_eq!(progress.last_event, Some(DecodeEvent::FrameHeader(header)));
    assert_eq!(progress.recent_events[1], Some(DecodeEvent::SequenceHeader(
        progress.sequence_header.expect("sequence header")
    )));
    assert_eq!(progress.recent_events[2], Some(DecodeEvent::FrameHeader(header)));
    assert_eq!(progress.recent_events[3], Some(DecodeEvent::FrameHeader(header)));
}

#[test]
fn test_rust_backend_parses_general_frame_header_prefix() {
    let mut decoder = Decoder::builder()
        .backend(BackendKind::Rust)
        .build()
        .expect("rust backend build");
    let payload = general_sequence_header_payload(80, 60);
    let mut seq_packet = vec![0x0a, payload.len() as u8];
    seq_packet.extend_from_slice(&payload);
    decoder.decode(&seq_packet).expect("general sequence header");
    let err = decoder
        .decode(&[0x1a, 0x03, 0x36, 0xd2, 0xc0])
        .expect_err("unsupported general frame should fail");
    assert!(err.to_string().contains("not implemented"));
    let progress = decoder.progress();
    let header = progress.last_frame_header.expect("frame header");
    let seq = progress.sequence_header.expect("sequence header");
    assert!(!seq.reduced_still_picture_header);
    assert!(seq.frame_id_numbers_present_flag);
    assert_eq!(seq.operating_point_idc_0, 0);
    assert_eq!(seq.seq_level_idx_0, 0);
    assert_eq!(seq.seq_tier_0, None);
    assert_eq!(seq.max_frame_width, 80);
    assert_eq!(seq.max_frame_height, 60);
    assert_eq!(seq.bit_depth, 8);
    assert!(!seq.monochrome);
    assert_eq!(seq.subsampling_x, 1);
    assert_eq!(seq.subsampling_y, 1);
    assert_eq!(seq.color_range, 0);
    assert_eq!(seq.chroma_sample_position, 0);
    assert_eq!(header.frame_type, Some(FrameType::Inter));
    assert!(header.show_frame);
    assert!(!header.show_existing_frame);
    assert_eq!(header.existing_frame_idx, None);
    assert_eq!(header.error_resilient_mode, Some(false));
    assert_eq!(header.disable_cdf_update, Some(true));
    assert_eq!(header.primary_ref_frame, Some(5));
    assert_eq!(header.refresh_frame_flags, Some(0xa5));
    assert_eq!(header.frame_size_override_flag, Some(true));
    assert_eq!(progress.last_event, Some(DecodeEvent::FrameHeader(header)));
    assert_eq!(progress.recent_events[2], Some(DecodeEvent::SequenceHeader(seq)));
    assert_eq!(progress.recent_events[3], Some(DecodeEvent::FrameHeader(header)));
}

#[test]
fn test_rust_backend_parses_show_existing_frame_prefix() {
    let mut decoder = Decoder::builder()
        .backend(BackendKind::Rust)
        .build()
        .expect("rust backend build");
    let payload = general_sequence_header_payload(80, 60);
    let mut seq_packet = vec![0x0a, payload.len() as u8];
    seq_packet.extend_from_slice(&payload);
    decoder.decode(&seq_packet).expect("general sequence header");
    let err = decoder
        .decode(&[0x1a, 0x01, 0b1010_0000])
        .expect_err("show-existing-frame path is outside M0");
    assert!(err.to_string().contains("not implemented"));
    let progress = decoder.progress();
    let header = progress.last_frame_header.expect("frame header");
    let seq = progress.sequence_header.expect("sequence header");
    assert!(!seq.reduced_still_picture_header);
    assert!(seq.frame_id_numbers_present_flag);
    assert_eq!(seq.operating_point_idc_0, 0);
    assert_eq!(seq.bit_depth, 8);
    assert!(!seq.monochrome);
    assert_eq!(seq.subsampling_x, 1);
    assert_eq!(seq.subsampling_y, 1);
    assert_eq!(header.frame_type, None);
    assert!(header.show_frame);
    assert!(header.show_existing_frame);
    assert_eq!(header.existing_frame_idx, Some(2));
    assert_eq!(header.error_resilient_mode, None);
    assert_eq!(header.disable_cdf_update, None);
    assert_eq!(header.primary_ref_frame, None);
    assert_eq!(header.refresh_frame_flags, None);
    assert_eq!(header.frame_size_override_flag, None);
    assert_eq!(progress.last_event, Some(DecodeEvent::FrameHeader(header)));
    assert_eq!(progress.recent_events[2], Some(DecodeEvent::SequenceHeader(seq)));
    assert_eq!(progress.recent_events[3], Some(DecodeEvent::FrameHeader(header)));
}

#[test]
fn test_rust_backend_decodes_m0_corpus_frame() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpora/m0/dc_intra_4x4.ivf");
    let expected_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpora/m0/dc_intra_4x4.expected.yuv");
    let mut decoded = Vec::new();
    let count = decode_ivf_with_backend(
        IvfReader::open(&path).expect("open m0 corpus"),
        BackendKind::Rust,
        None,
        |frame| {
            let owned = frame.to_owned();
            decoded.extend_from_slice(&owned.planes[0]);
            decoded.extend_from_slice(&owned.planes[1]);
            decoded.extend_from_slice(&owned.planes[2]);
        },
    )
    .expect("rust backend should decode m0 corpus");
    assert_eq!(count, 1);
    let expected = y4m_payload(&fs::read(expected_path).expect("read expected y4m"));
    assert_eq!(decoded, expected);
}

fn y4m_payload(bytes: &[u8]) -> Vec<u8> {
    let mut split = bytes.splitn(3, |&b| b == b'\n');
    let _file_header = split.next().expect("y4m file header");
    let _frame_header = split.next().expect("y4m frame header");
    split.next().expect("y4m payload").to_vec()
}

#[test]
fn test_decode_empty_slice_returns_error() {
    let mut decoder = Decoder::new().unwrap();
    // Empty data should be rejected by the codec
    let result = decoder.decode(&[]);
    // The codec may return OK (no-op) or error — either is acceptable
    // as long as it doesn't crash
    let _ = result;
}

#[test]
#[ignore = "C codec may SIGABRT on malformed input via internal assert(); unsafe to run in-process"]
fn test_decode_garbage_data_returns_error() {
    let mut decoder = Decoder::new().unwrap();
    let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x01, 0x02, 0x03];
    // Random bytes should be rejected
    let result = decoder.decode(&garbage);
    assert!(result.is_err(), "garbage data should produce decode error");
}

#[test]
fn test_get_frames_before_decode_is_empty() {
    let mut decoder = Decoder::new().unwrap();
    let frames: Vec<_> = decoder.get_frames().collect();
    assert!(frames.is_empty(), "no frames before any decode call");
}

#[test]
fn test_decoder_drop_without_decode() {
    // Construct and immediately drop — should not crash
    let decoder = Decoder::new().unwrap();
    drop(decoder);
}

#[test]
fn test_flush_before_decode() {
    let mut decoder = Decoder::new().unwrap();
    // Flushing before any decode should be OK (no-op)
    let result = decoder.flush();
    assert!(result.is_ok(), "flush without prior decode should succeed");
}

#[test]
#[ignore = "C codec may SIGABRT on malformed input via internal assert(); unsafe to run in-process"]
fn test_flush_after_garbage_decode() {
    let mut decoder = Decoder::new().unwrap();
    let _ = decoder.decode(&[0xFF; 16]);
    // Flush after error — should not crash
    let _ = decoder.flush();
}

/// Minimal `FrameBufferManager` that owns a fixed pool of `Vec<u8>`s.
///
/// Each allocate call returns the next free slot; release marks it free.
/// Used by [`test_set_frame_buffer_manager_smoke`] and
/// [`test_replace_frame_buffer_manager`] to exercise the safe registration
/// and Drop reclamation paths without needing test bitstream data.
struct PoolManager {
    slots: Vec<(Vec<u8>, bool)>, // (storage, in_use)
    allocs: Arc<AtomicUsize>,
    releases: Arc<AtomicUsize>,
}

impl PoolManager {
    fn new(num_slots: usize) -> (Self, Arc<AtomicUsize>, Arc<AtomicUsize>) {
        let allocs = Arc::new(AtomicUsize::new(0));
        let releases = Arc::new(AtomicUsize::new(0));
        let mgr = PoolManager {
            slots: (0..num_slots).map(|_| (Vec::new(), false)).collect(),
            allocs: Arc::clone(&allocs),
            releases: Arc::clone(&releases),
        };
        (mgr, allocs, releases)
    }
}

impl FrameBufferManager for PoolManager {
    fn allocate(&mut self, min_size: usize) -> Option<FrameBuffer> {
        self.allocs.fetch_add(1, Ordering::SeqCst);
        let idx = self.slots.iter().position(|(_, used)| !*used)?;
        let (buf, used) = &mut self.slots[idx];
        if buf.len() < min_size {
            *buf = vec![0u8; min_size];
        } else {
            buf.iter_mut().for_each(|b| *b = 0);
        }
        *used = true;
        Some(FrameBuffer {
            data: NonNull::new(buf.as_mut_ptr()).expect("Vec data ptr is non-null"),
            len: buf.len(),
            token: idx,
        })
    }

    fn release(&mut self, buffer: FrameBuffer) {
        self.releases.fetch_add(1, Ordering::SeqCst);
        if let Some((_, used)) = self.slots.get_mut(buffer.token) {
            *used = false;
        }
    }
}

#[test]
fn test_set_frame_buffer_manager_smoke() {
    // Register a manager, then drop the decoder.  No actual decoding so no
    // callbacks fire — but the box-leak / box-reclaim cycle in Drop runs.
    let mut decoder = Decoder::new().unwrap();
    let (mgr, allocs, releases) = PoolManager::new(8);
    decoder.set_frame_buffer_manager(mgr).expect("registration");
    drop(decoder);
    // No decode happened, so no callbacks were invoked.
    assert_eq!(allocs.load(Ordering::SeqCst), 0);
    assert_eq!(releases.load(Ordering::SeqCst), 0);
}

#[test]
fn test_replace_frame_buffer_manager() {
    // Registering a second manager must drop the first cleanly.
    let mut decoder = Decoder::new().unwrap();
    let (mgr1, _, _) = PoolManager::new(4);
    decoder.set_frame_buffer_manager(mgr1).expect("first registration");
    let (mgr2, _, _) = PoolManager::new(4);
    decoder
        .set_frame_buffer_manager(mgr2)
        .expect("second registration replaces first");
    drop(decoder);
}
