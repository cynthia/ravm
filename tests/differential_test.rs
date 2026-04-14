use rustavm::backend::BackendKind;
use rustavm::diff::{
    compare_ivf_file, compare_ivf_file_outcomes, compare_outcome_overlap, compare_snapshots,
    decode_ivf_outcome, decode_ivf_snapshot,
};
use rustavm::ivf::IvfReader;
use rustavm::stream::{decode_ivf_reader_with_backend, StreamError};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

fn empty_ivf() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(32);
    bytes.extend_from_slice(b"DKIF");
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(b"AV02");
    bytes.extend_from_slice(&64u16.to_le_bytes());
    bytes.extend_from_slice(&64u16.to_le_bytes());
    bytes.extend_from_slice(&30u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes
}

fn ivf_with_one_frame(payload: &[u8]) -> Vec<u8> {
    let mut bytes = empty_ivf();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&0u64.to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn frame_obu_packet() -> Vec<u8> {
    vec![0x32, 0x00]
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
    bits.push_bits(0, 5);
    bits.push_bits(1, 1);
    bits.push_bits(1, 1);
    bits.push_bits(0, 5);
    bits.push_bits(width_bits - 1, 4);
    bits.push_bits(height_bits - 1, 4);
    bits.push_bits(width_minus_1, width_bits as u8);
    bits.push_bits(height_minus_1, height_bits as u8);
    bits.push_bits(0, 1); // high_bitdepth => 8-bit
    bits.push_bits(0, 1); // monochrome => false
    bits.push_bits(0, 1); // color_range => studio
    bits.into_bytes()
}

fn packet_with_sequence_header_and_frame(width: u32, height: u32) -> Vec<u8> {
    let seq = reduced_still_sequence_header_payload(width, height);
    let mut packet = vec![0x0a, seq.len() as u8];
    packet.extend_from_slice(&seq);
    packet.extend_from_slice(&frame_obu_packet());
    packet
}

fn bundled_sample_ivf() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tools/avm_analyzer/avm_analyzer_app/assets/leo_qcif.ivf")
}

#[test]
fn differential_harness_accepts_identical_snapshots() {
    let data = empty_ivf();
    let left = decode_ivf_snapshot(
        IvfReader::new(Cursor::new(data.clone())).expect("ivf header"),
        BackendKind::Libavm,
        None,
    )
    .expect("decode snapshot");
    let right = decode_ivf_snapshot(
        IvfReader::new(Cursor::new(data)).expect("ivf header"),
        BackendKind::Libavm,
        None,
    )
    .expect("decode snapshot");

    compare_snapshots(&left, &right).expect("identical snapshots");
    assert_eq!(left.progress.backend, BackendKind::Libavm);
    assert_eq!(left.progress.packets_parsed, Some(0));
    assert_eq!(left.progress.stream_info, None);
}

#[test]
fn rust_backend_reports_truncated_frame_packet_error() {
    let data = ivf_with_one_frame(&frame_obu_packet());
    let err = decode_ivf_snapshot(
        IvfReader::new(Cursor::new(data)).expect("ivf header"),
        BackendKind::Rust,
        None,
    )
    .expect_err("rust backend should reject truncated frame packet");
    assert!(err.to_string().contains("frame"));
}

#[test]
fn rust_backend_outcome_captures_terminal_progress() {
    let data = ivf_with_one_frame(&frame_obu_packet());
    let outcome = decode_ivf_outcome(
        IvfReader::new(Cursor::new(data)).expect("ivf header"),
        BackendKind::Rust,
        None,
    )
    .expect("decode outcome");
    assert_eq!(outcome.snapshot.backend, BackendKind::Rust);
    assert_eq!(outcome.stopped_at_packet, Some(0));
    assert!(
        outcome
            .terminal_error
            .as_deref()
            .expect("terminal error")
            .contains("frame")
    );
    assert_eq!(outcome.snapshot.progress.packets_parsed, Some(1));
}

#[test]
fn rust_backend_outcome_keeps_stream_info_on_same_packet_failure() {
    let data = ivf_with_one_frame(&packet_with_sequence_header_and_frame(64, 48));
    let outcome = decode_ivf_outcome(
        IvfReader::new(Cursor::new(data)).expect("ivf header"),
        BackendKind::Rust,
        None,
    )
    .expect("decode outcome");
    let info = outcome.snapshot.stream_info.expect("stream info");
    assert_eq!(info.width, 64);
    assert_eq!(info.height, 48);
    assert_eq!(outcome.stopped_at_packet, Some(0));
    assert_eq!(outcome.snapshot.progress.stream_info, Some(info));
    assert_eq!(outcome.snapshot.progress.packets_parsed, Some(1));
}

#[test]
fn compare_ivf_file_accepts_identical_backends() {
    let path = std::env::temp_dir().join(format!(
        "rustavm-empty-ivf-{}-{}.ivf",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ));
    fs::write(&path, empty_ivf()).expect("write ivf");
    let result = compare_ivf_file(&path, BackendKind::Libavm, BackendKind::Libavm, None);
    fs::remove_file(&path).expect("cleanup");
    let (left, right) = result.expect("compare");
    assert_eq!(left.frames.len(), 0);
    assert_eq!(right.frames.len(), 0);
}

#[test]
fn compare_ivf_file_outcomes_accepts_identical_terminal_failures() {
    let path = std::env::temp_dir().join(format!(
        "rustavm-rust-fail-ivf-{}-{}.ivf",
        std::process::id(),
        std::thread::current().name().unwrap_or("main")
    ));
    fs::write(&path, ivf_with_one_frame(&frame_obu_packet())).expect("write ivf");
    let result = compare_ivf_file_outcomes(&path, BackendKind::Rust, BackendKind::Rust, None);
    fs::remove_file(&path).expect("cleanup");
    let (left, right) = result.expect("compare outcomes");
    assert_eq!(left.stopped_at_packet, Some(0));
    assert_eq!(right.stopped_at_packet, Some(0));
    assert_eq!(left.snapshot.progress, right.snapshot.progress);
}

#[test]
fn bundled_sample_exposes_cross_backend_outcomes() {
    let path = bundled_sample_ivf();
    assert!(path.is_file(), "missing bundled sample: {}", path.display());

    let rust = decode_ivf_outcome(
        IvfReader::open(&path).expect("open bundled sample"),
        BackendKind::Rust,
        None,
    )
    .expect("rust outcome");
    let libavm = decode_ivf_outcome(
        IvfReader::open(&path).expect("open bundled sample"),
        BackendKind::Libavm,
        None,
    )
    .expect("libavm outcome");

    assert_eq!(rust.snapshot.backend, BackendKind::Rust);
    assert_eq!(libavm.snapshot.backend, BackendKind::Libavm);
    assert_eq!(rust.stopped_at_packet, Some(0));
    assert_eq!(libavm.stopped_at_packet, Some(0));
    assert_eq!(rust.snapshot.progress.packets_parsed, Some(1));
    assert_eq!(libavm.snapshot.progress.packets_parsed, Some(1));
    assert!(
        rust.terminal_error
            .as_deref()
            .expect("rust terminal error")
            .contains("frame")
    );
    assert!(
        libavm
            .terminal_error
            .as_deref()
            .expect("libavm terminal error")
            .contains("generic error")
    );
    assert_eq!(libavm.snapshot.stream_info, None);
    assert_eq!(libavm.snapshot.progress.stream_info, None);
    compare_outcome_overlap(&rust, &libavm).expect("cross-backend overlap");
}

#[test]
fn backend_stream_helper_reports_progress_on_rust_error() {
    let data = ivf_with_one_frame(&[0x32, 0x00]);
    let err = decode_ivf_reader_with_backend(Cursor::new(data), BackendKind::Rust, None, |_| {})
        .expect_err("rust backend should stop at frame packet");
    match err {
        StreamError::DecoderAtPacket {
            packet_index,
            progress,
            source,
        } => {
            assert_eq!(packet_index, 0);
            assert_eq!(progress.backend, BackendKind::Rust);
            assert_eq!(progress.packets_parsed, Some(1));
            assert!(source.to_string().contains("frame"));
        }
        other => panic!("unexpected error variant: {other}"),
    }
}
