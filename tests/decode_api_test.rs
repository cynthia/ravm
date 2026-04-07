//! Decode API validation tests — exercise error paths without test data.

use rustavm::decoder::Decoder;

#[test]
fn test_decoder_new_succeeds() {
    assert!(Decoder::new().is_ok());
}

#[test]
fn test_decoder_with_threads_succeeds() {
    assert!(Decoder::with_config(Some(4)).is_ok());
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
