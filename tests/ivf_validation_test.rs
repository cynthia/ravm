//! Tests for IVF reader validation (R1 frame size cap, R2 header validation).

use std::io::{self, Cursor};

use rustavm::ivf::IvfReader;

// IVF file structure:
// - 4 bytes: "DKIF" signature
// - 28 bytes: header (version u16, header_len u16, fourcc 4B, width u16, height u16,
//   framerate_num u32, framerate_den u32, num_frames u32, unused 4B)
// - Per frame: size u32, timestamp u64, data [u8; size]

/// Build a minimal valid IVF header (32 bytes total: 4 sig + 28 header).
fn valid_ivf_header(width: u16, height: u16) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"DKIF");             // signature
    buf.extend_from_slice(&0u16.to_le_bytes()); // version = 0
    buf.extend_from_slice(&32u16.to_le_bytes()); // header_length = 32
    buf.extend_from_slice(b"AV01");             // fourcc
    buf.extend_from_slice(&width.to_le_bytes());
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(&30u32.to_le_bytes()); // framerate_num
    buf.extend_from_slice(&1u32.to_le_bytes());  // framerate_den
    buf.extend_from_slice(&1u32.to_le_bytes());  // num_frames
    buf.extend_from_slice(&[0u8; 4]);            // unused
    buf
}

/// Append a frame header (size + timestamp) and data to a buffer.
fn append_frame(buf: &mut Vec<u8>, data: &[u8], timestamp: u64) {
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    buf.extend_from_slice(&timestamp.to_le_bytes());
    buf.extend_from_slice(data);
}

// ---------------------------------------------------------------------------
// Header parsing tests (R2)
// ---------------------------------------------------------------------------

#[test]
fn test_valid_ivf_parses_ok() {
    let mut buf = valid_ivf_header(320, 240);
    append_frame(&mut buf, &[0u8; 64], 0);

    let mut reader = IvfReader::new(Cursor::new(buf)).expect("valid header should parse");
    let frame = reader.next_frame().expect("next_frame should not error");
    assert!(frame.is_some(), "expected a frame");
    let frame = frame.unwrap();
    assert_eq!(frame.data.len(), 64);
    assert_eq!(frame.timestamp, 0);
}

#[test]
fn test_invalid_signature() {
    let mut buf = valid_ivf_header(320, 240);
    // Overwrite the 4-byte signature with garbage.
    buf[0..4].copy_from_slice(b"XXXX");

    let err = match IvfReader::new(Cursor::new(buf)) {
        Ok(_) => panic!("expected Err for bad signature"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn test_unsupported_version() {
    let mut buf = valid_ivf_header(320, 240);
    // version is at bytes 4-5 (after the 4-byte "DKIF" signature).
    buf[4..6].copy_from_slice(&1u16.to_le_bytes());

    let err = match IvfReader::new(Cursor::new(buf)) {
        Ok(_) => panic!("expected Err for version != 0"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("version"),
        "error should mention version, got: {err}"
    );
}

#[test]
fn test_unexpected_header_length() {
    let mut buf = valid_ivf_header(320, 240);
    // header_len is at bytes 6-7.
    buf[6..8].copy_from_slice(&64u16.to_le_bytes());

    let err = match IvfReader::new(Cursor::new(buf)) {
        Ok(_) => panic!("expected Err for wrong header_len"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("length") || err.to_string().contains("64"),
        "error should mention length, got: {err}"
    );
}

#[test]
fn test_zero_width_rejected() {
    // width=0, height=100
    let buf = valid_ivf_header(0, 100);

    let err = match IvfReader::new(Cursor::new(buf)) {
        Ok(_) => panic!("expected Err for zero width"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("zero") || err.to_string().contains("dimension"),
        "error should mention zero dimension, got: {err}"
    );
}

#[test]
fn test_zero_height_rejected() {
    // width=100, height=0
    let buf = valid_ivf_header(100, 0);

    let err = match IvfReader::new(Cursor::new(buf)) {
        Ok(_) => panic!("expected Err for zero height"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

// ---------------------------------------------------------------------------
// Frame size limit tests (R1)
// ---------------------------------------------------------------------------

#[test]
fn test_frame_size_exceeds_default_limit() {
    // Default limit is 64 MiB. A declared size of u32::MAX far exceeds it.
    // We only need to write the 4-byte size field; the limit check fires before
    // the timestamp/data reads.
    let mut buf = valid_ivf_header(320, 240);
    buf.extend_from_slice(&u32::MAX.to_le_bytes()); // frame size = 4 GiB - 1

    let mut reader = IvfReader::new(Cursor::new(buf)).expect("header should parse");
    let err = match reader.next_frame() {
        Ok(_) => panic!("expected Err for oversized frame"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    assert!(
        err.to_string().contains("limit") || err.to_string().contains("size"),
        "error should mention limit, got: {err}"
    );
}

#[test]
fn test_frame_size_within_limit() {
    let mut buf = valid_ivf_header(320, 240);
    append_frame(&mut buf, &[0xABu8; 100], 42);

    let mut reader = IvfReader::new(Cursor::new(buf)).expect("header should parse");
    let frame = reader
        .next_frame()
        .expect("small frame should not error")
        .expect("expected Some frame");
    assert_eq!(frame.data.len(), 100);
    assert_eq!(frame.timestamp, 42);
}

#[test]
fn test_custom_max_frame_size() {
    // Set a 50-byte cap, then send a 100-byte frame — must be rejected.
    let mut buf = valid_ivf_header(320, 240);
    append_frame(&mut buf, &[0u8; 100], 0);

    let mut reader = IvfReader::with_max_frame_size(Cursor::new(buf), 50)
        .expect("header should parse");
    let err = match reader.next_frame() {
        Ok(_) => panic!("expected Err for frame exceeding custom cap"),
        Err(e) => e,
    };
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn test_eof_returns_none() {
    // Header only, no frame bytes — clean EOF on the first read in next_frame.
    let buf = valid_ivf_header(320, 240);

    let mut reader = IvfReader::new(Cursor::new(buf)).expect("header should parse");
    let result = reader.next_frame().expect("EOF should return Ok(None)");
    assert!(result.is_none(), "expected None at end of stream");
}
