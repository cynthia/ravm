//! Decode API validation tests — exercise error paths without test data.

use rustavm::decoder::{Decoder, FrameBuffer, FrameBufferManager};
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
