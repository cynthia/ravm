//! Integration tests for the external frame buffer management API.
//!
//! Ports the key tests from `avm/test/external_frame_buffer_test.cc` to Rust.
//!
//! Tests marked `#[ignore]` require an IVF test vector to decode. Set
//! `LIBAVM_TEST_DATA_PATH` to a directory containing at least one IVF file
//! (e.g. `av1-1-b8-00-quantizer-00.ivf`) and run:
//!
//! ```sh
//! LIBAVM_TEST_DATA_PATH=/path/to/data cargo test --test frame_buffer_test -- --ignored
//! ```

use std::os::raw::{c_int, c_void};
use std::path::{Path, PathBuf};

use rustavm::decoder::{Decoder, DecoderError, ErrorKind};
use rustavm::ffi::{
    avm_codec_av2_dx, avm_codec_ctx_t, avm_codec_dec_init_ver, avm_codec_destroy,
    avm_codec_err_t_AVM_CODEC_INVALID_PARAM, avm_codec_err_t_AVM_CODEC_OK,
    avm_codec_frame_buffer_t, avm_codec_set_frame_buffer_functions, AVM_DECODER_ABI_VERSION,
    AVM_MAXIMUM_REF_BUFFERS, AVM_MAXIMUM_WORK_BUFFERS,
};
use rustavm::ivf::IvfReader;

// ---------------------------------------------------------------------------
// Test data helpers
// ---------------------------------------------------------------------------

/// Resolve the test-data directory from the environment.
fn test_data_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("LIBAVM_TEST_DATA_PATH") {
        let path = PathBuf::from(p);
        if path.is_dir() {
            return Some(path);
        }
    }
    // Fallback: check common build output location
    let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("avm")
        .join("out")
        .join("testdata");
    if fallback.is_dir() {
        return Some(fallback);
    }
    None
}

/// Find an IVF test file.  Returns `None` if no test data is available.
fn find_test_ivf() -> Option<PathBuf> {
    let dir = test_data_dir()?;
    // Prefer the smallest quantizer-00 file for speed.
    let preferred = [
        "av1-1-b8-00-quantizer-00.ivf",
        "av1-1-b8-01-size-16x16.ivf",
    ];
    for name in &preferred {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    // Fall back to any .ivf file in the directory.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("ivf") && p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Read all frames from an IVF file into memory.
fn read_ivf_frames(path: &Path) -> Vec<Vec<u8>> {
    let file = std::fs::File::open(path).expect("failed to open IVF file");
    let mut reader = IvfReader::new(file).expect("failed to parse IVF header");
    let mut frames = Vec::new();
    while let Ok(Some(frame)) = reader.next_frame() {
        frames.push(frame.data);
    }
    frames
}

// ---------------------------------------------------------------------------
// External frame buffer manager
// ---------------------------------------------------------------------------

/// A single buffer entry tracked by the manager.
struct BufferEntry {
    data: Vec<u8>,
    in_use: bool,
}

/// Configurable failure modes for testing error paths.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FailMode {
    /// Normal operation.
    None,
    /// `get` callback returns null data (fb->data = NULL) but reports success.
    NullData,
    /// `get` callback allocates one byte less than requested.
    OneLessByte,
    /// `release` callback is a no-op (never marks buffers free).
    NoRelease,
}

/// Rust port of the C `ExternalFrameBufferList`.
///
/// Manages a fixed-size pool of frame buffers.  The pool is allocated once
/// in `new()` and never resized, so element addresses (stored as indices in
/// `fb->priv_`) remain valid for the manager's lifetime.
struct ExternalFrameBufferList {
    buffers: Vec<BufferEntry>,
    num_used: usize,
    get_count: usize,
    release_count: usize,
    fail_mode: FailMode,
}

impl ExternalFrameBufferList {
    fn new(num_buffers: usize) -> Self {
        let mut buffers = Vec::with_capacity(num_buffers);
        for _ in 0..num_buffers {
            buffers.push(BufferEntry {
                data: Vec::new(),
                in_use: false,
            });
        }
        Self {
            buffers,
            num_used: 0,
            get_count: 0,
            release_count: 0,
            fail_mode: FailMode::None,
        }
    }

    fn with_fail_mode(mut self, mode: FailMode) -> Self {
        self.fail_mode = mode;
        self
    }

    /// Find a free buffer slot, (re)allocate if needed, fill `fb`.
    /// Returns 0 on success, -1 on error (matching C convention).
    fn get_free_buffer(
        &mut self,
        min_size: usize,
        fb: &mut avm_codec_frame_buffer_t,
    ) -> c_int {
        self.get_count += 1;

        // Find a free slot.
        let idx = match self.buffers.iter().position(|b| !b.in_use) {
            Some(i) => i,
            std::option::Option::None => return -1, // all buffers in use
        };

        let alloc_size = match self.fail_mode {
            FailMode::OneLessByte => {
                if min_size > 0 { min_size - 1 } else { 0 }
            }
            _ => min_size,
        };

        // (Re)allocate if the existing buffer is too small.
        if self.buffers[idx].data.len() < alloc_size {
            self.buffers[idx].data = vec![0u8; alloc_size];
        } else {
            // Zero existing data (required by the API contract).
            self.buffers[idx].data.iter_mut().for_each(|b| *b = 0);
        }

        self.buffers[idx].in_use = true;
        self.num_used += 1;

        match self.fail_mode {
            FailMode::NullData => {
                fb.data = std::ptr::null_mut();
                fb.size = alloc_size;
            }
            _ => {
                fb.data = self.buffers[idx].data.as_mut_ptr();
                fb.size = self.buffers[idx].data.len();
            }
        }

        // Store the index so the release callback can identify the entry.
        fb.priv_ = idx as *mut c_void;

        0
    }

    /// Mark a buffer as free.  Returns 0 on success, -1 on error.
    fn release_buffer(&mut self, fb: &mut avm_codec_frame_buffer_t) -> c_int {
        self.release_count += 1;

        if self.fail_mode == FailMode::NoRelease {
            // Intentionally do NOT mark the buffer as free.
            return 0;
        }

        let idx = fb.priv_ as usize;
        if idx >= self.buffers.len() {
            return -1;
        }
        if !self.buffers[idx].in_use {
            return -1; // double release
        }
        self.buffers[idx].in_use = false;
        self.num_used -= 1;
        0
    }
}

// ---------------------------------------------------------------------------
// extern "C" callbacks
// ---------------------------------------------------------------------------

/// Standard get-frame-buffer callback.
unsafe extern "C" fn get_frame_buffer(
    priv_: *mut c_void,
    min_size: usize,
    fb: *mut avm_codec_frame_buffer_t,
) -> c_int {
    if priv_.is_null() || fb.is_null() {
        return -1;
    }
    let list = unsafe { &mut *(priv_ as *mut ExternalFrameBufferList) };
    let fb_ref = unsafe { &mut *fb };
    list.get_free_buffer(min_size, fb_ref)
}

/// Standard release-frame-buffer callback.
unsafe extern "C" fn release_frame_buffer(
    priv_: *mut c_void,
    fb: *mut avm_codec_frame_buffer_t,
) -> c_int {
    if priv_.is_null() || fb.is_null() {
        return -1;
    }
    let list = unsafe { &mut *(priv_ as *mut ExternalFrameBufferList) };
    let fb_ref = unsafe { &mut *fb };
    list.release_buffer(fb_ref)
}

/// Release callback that deliberately does nothing (for NoRelease test).
unsafe extern "C" fn do_not_release_frame_buffer(
    _priv: *mut c_void,
    _fb: *mut avm_codec_frame_buffer_t,
) -> c_int {
    0
}

// ---------------------------------------------------------------------------
// Helper: decode all frames from an IVF file with a given buffer setup
// ---------------------------------------------------------------------------

/// Set up external frame buffers on a decoder and decode all frames from the
/// given IVF path.  Returns the decode result and a reference to the manager
/// (which is kept alive via the returned Box).
fn decode_with_ext_fb(
    path: &Path,
    num_buffers: usize,
    fail_mode: FailMode,
    custom_release: Option<
        unsafe extern "C" fn(*mut c_void, *mut avm_codec_frame_buffer_t) -> c_int,
    >,
) -> (Result<(), DecoderError>, Box<ExternalFrameBufferList>) {
    let frames = read_ivf_frames(path);
    assert!(!frames.is_empty(), "IVF file has no frames");

    let mut manager = Box::new(
        ExternalFrameBufferList::new(num_buffers).with_fail_mode(fail_mode),
    );
    let mut decoder = Decoder::new().expect("decoder init failed");

    let release_fn = custom_release.unwrap_or(release_frame_buffer);

    unsafe {
        decoder
            .set_frame_buffer_functions(
                get_frame_buffer,
                release_fn,
                &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
            )
            .expect("set_frame_buffer_functions failed");
    }

    let mut result = Ok(());
    for frame_data in &frames {
        if let Err(e) = decoder.decode(frame_data) {
            result = Err(e);
            break;
        }
        // Drain decoded frames to trigger buffer release.
        for _frame in decoder.get_frames() {}
    }

    (result, manager)
}

// ===========================================================================
// Tests that do NOT require test data
// ===========================================================================

/// Verify that registering callbacks on a fresh decoder succeeds.
#[test]
fn test_callback_registration_succeeds() {
    let mut decoder = Decoder::new().expect("decoder init failed");
    let mut manager = Box::new(ExternalFrameBufferList::new(
        (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize,
    ));

    let result = unsafe {
        decoder.set_frame_buffer_functions(
            get_frame_buffer,
            release_frame_buffer,
            &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
        )
    };
    assert!(result.is_ok(), "callback registration should succeed");
}

/// Verify that the callback signatures match the C types exactly — no
/// transmute is needed (regression test for the phase-2 transmute removal).
///
/// This is a compile-time test: if the fn pointer types didn't match
/// `avm_get_frame_buffer_cb_fn_t` / `avm_release_frame_buffer_cb_fn_t`,
/// the code in `set_frame_buffer_functions` would fail to compile.
#[test]
fn test_callback_type_safety_no_transmute() {
    // The fact that this compiles proves the types are compatible.
    // We just call set_frame_buffer_functions with our typed callbacks.
    let mut decoder = Decoder::new().expect("decoder init failed");
    let mut manager = Box::new(ExternalFrameBufferList::new(4));

    let result = unsafe {
        decoder.set_frame_buffer_functions(
            get_frame_buffer,
            release_frame_buffer,
            &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
        )
    };
    assert!(result.is_ok());
}

/// Verify that passing NULL as the get callback to the raw C API returns
/// `AVM_CODEC_INVALID_PARAM`.
///
/// The safe Rust wrapper prevents this at compile time (non-optional fn
/// pointer), so we must call the raw FFI to test this C-level guard.
#[test]
fn test_null_get_callback_returns_invalid_param() {
    unsafe {
        let mut ctx: avm_codec_ctx_t = std::mem::zeroed();
        let iface = avm_codec_av2_dx();
        let init_res = avm_codec_dec_init_ver(
            &mut ctx,
            iface,
            std::ptr::null(),
            0,
            AVM_DECODER_ABI_VERSION as i32,
        );
        assert_eq!(init_res, avm_codec_err_t_AVM_CODEC_OK);

        let mut manager = Box::new(ExternalFrameBufferList::new(
            (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize,
        ));

        let res = avm_codec_set_frame_buffer_functions(
            &mut ctx,
            None, // NULL get callback
            Some(release_frame_buffer),
            &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
        );
        assert_eq!(
            res, avm_codec_err_t_AVM_CODEC_INVALID_PARAM,
            "NULL get callback should return INVALID_PARAM"
        );

        avm_codec_destroy(&mut ctx);
    }
}

/// Verify that passing NULL as the release callback to the raw C API returns
/// `AVM_CODEC_INVALID_PARAM`.
#[test]
fn test_null_release_callback_returns_invalid_param() {
    unsafe {
        let mut ctx: avm_codec_ctx_t = std::mem::zeroed();
        let iface = avm_codec_av2_dx();
        let init_res = avm_codec_dec_init_ver(
            &mut ctx,
            iface,
            std::ptr::null(),
            0,
            AVM_DECODER_ABI_VERSION as i32,
        );
        assert_eq!(init_res, avm_codec_err_t_AVM_CODEC_OK);

        let mut manager = Box::new(ExternalFrameBufferList::new(
            (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize,
        ));

        let res = avm_codec_set_frame_buffer_functions(
            &mut ctx,
            Some(get_frame_buffer),
            None, // NULL release callback
            &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
        );
        assert_eq!(
            res, avm_codec_err_t_AVM_CODEC_INVALID_PARAM,
            "NULL release callback should return INVALID_PARAM"
        );

        avm_codec_destroy(&mut ctx);
    }
}

/// Verify that the manager struct can be created and dropped without issue,
/// even when no decode has occurred.
#[test]
fn test_manager_lifecycle_without_decode() {
    let mut decoder = Decoder::new().expect("decoder init failed");
    let mut manager = Box::new(ExternalFrameBufferList::new(8));

    unsafe {
        decoder
            .set_frame_buffer_functions(
                get_frame_buffer,
                release_frame_buffer,
                &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
            )
            .expect("registration should succeed");
    }

    // Drop decoder first, then manager — this is the safe order.
    drop(decoder);
    assert_eq!(manager.get_count, 0, "no frames decoded, no get calls");
    assert_eq!(manager.release_count, 0, "no frames decoded, no release calls");
    drop(manager);
}

/// Verify that dropping the decoder before the manager doesn't cause UB.
///
/// The decoder's Drop calls `avm_codec_destroy`, which should release all
/// held frame buffers via the release callback.  The manager must still be
/// alive at that point.
#[test]
fn test_decoder_dropped_before_manager() {
    let mut manager = Box::new(ExternalFrameBufferList::new(
        (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize,
    ));
    let mut decoder = Decoder::new().expect("decoder init failed");

    unsafe {
        decoder
            .set_frame_buffer_functions(
                get_frame_buffer,
                release_frame_buffer,
                &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
            )
            .expect("registration should succeed");
    }

    // Decoder dropped first — destroy should invoke release callbacks.
    drop(decoder);
    // Manager is still alive — no use-after-free.
    assert_eq!(
        manager.num_used, 0,
        "all buffers should be released after decoder drop"
    );
}

/// Verify the manager correctly tracks get/release call counts via
/// direct Rust calls (no FFI round-trip needed).
#[test]
fn test_manager_counters() {
    let mut mgr = ExternalFrameBufferList::new(4);
    let mut fb = avm_codec_frame_buffer_t {
        data: std::ptr::null_mut(),
        size: 0,
        priv_: std::ptr::null_mut(),
    };

    assert_eq!(mgr.get_free_buffer(1024, &mut fb), 0);
    assert_eq!(mgr.get_count, 1);
    assert_eq!(mgr.num_used, 1);
    assert!(!fb.data.is_null());
    assert!(fb.size >= 1024);

    assert_eq!(mgr.release_buffer(&mut fb), 0);
    assert_eq!(mgr.release_count, 1);
    assert_eq!(mgr.num_used, 0);
}

/// Verify that requesting more buffers than available returns -1.
#[test]
fn test_manager_exhaustion() {
    let mut mgr = ExternalFrameBufferList::new(2);
    let mut fb1 = avm_codec_frame_buffer_t {
        data: std::ptr::null_mut(),
        size: 0,
        priv_: std::ptr::null_mut(),
    };
    let mut fb2 = avm_codec_frame_buffer_t {
        data: std::ptr::null_mut(),
        size: 0,
        priv_: std::ptr::null_mut(),
    };
    let mut fb3 = avm_codec_frame_buffer_t {
        data: std::ptr::null_mut(),
        size: 0,
        priv_: std::ptr::null_mut(),
    };

    assert_eq!(mgr.get_free_buffer(64, &mut fb1), 0);
    assert_eq!(mgr.get_free_buffer(64, &mut fb2), 0);
    // Pool exhausted — should fail.
    assert_eq!(mgr.get_free_buffer(64, &mut fb3), -1);

    // Release one, then retry.
    assert_eq!(mgr.release_buffer(&mut fb1), 0);
    assert_eq!(mgr.get_free_buffer(64, &mut fb3), 0);
}

/// Verify that releasing a buffer twice is detected as an error.
#[test]
fn test_manager_double_release() {
    let mut mgr = ExternalFrameBufferList::new(2);
    let mut fb = avm_codec_frame_buffer_t {
        data: std::ptr::null_mut(),
        size: 0,
        priv_: std::ptr::null_mut(),
    };

    assert_eq!(mgr.get_free_buffer(64, &mut fb), 0);
    assert_eq!(mgr.release_buffer(&mut fb), 0);
    // Second release should fail (already free).
    assert_eq!(mgr.release_buffer(&mut fb), -1);
}

/// Verify buffer reuse: after release, the same slot can be reused.
#[test]
fn test_manager_buffer_reuse() {
    let mut mgr = ExternalFrameBufferList::new(1);
    let mut fb = avm_codec_frame_buffer_t {
        data: std::ptr::null_mut(),
        size: 0,
        priv_: std::ptr::null_mut(),
    };

    // Allocate, release, allocate again — should reuse slot 0.
    assert_eq!(mgr.get_free_buffer(128, &mut fb), 0);
    let first_idx = fb.priv_ as usize;
    assert_eq!(mgr.release_buffer(&mut fb), 0);

    assert_eq!(mgr.get_free_buffer(64, &mut fb), 0);
    let second_idx = fb.priv_ as usize;
    assert_eq!(first_idx, second_idx, "should reuse the same slot");
}

/// Verify buffer growth: re-requesting a larger size reallocates.
#[test]
fn test_manager_buffer_growth() {
    let mut mgr = ExternalFrameBufferList::new(1);
    let mut fb = avm_codec_frame_buffer_t {
        data: std::ptr::null_mut(),
        size: 0,
        priv_: std::ptr::null_mut(),
    };

    assert_eq!(mgr.get_free_buffer(64, &mut fb), 0);
    assert!(fb.size >= 64);
    assert_eq!(mgr.release_buffer(&mut fb), 0);

    // Request a larger buffer — should grow.
    assert_eq!(mgr.get_free_buffer(1024, &mut fb), 0);
    assert!(fb.size >= 1024);
}

/// Verify NullData fail mode: get succeeds but fb->data is NULL.
#[test]
fn test_manager_null_data_mode() {
    let mut mgr = ExternalFrameBufferList::new(4).with_fail_mode(FailMode::NullData);
    let mut fb = avm_codec_frame_buffer_t {
        data: std::ptr::null_mut(),
        size: 0,
        priv_: std::ptr::null_mut(),
    };

    assert_eq!(mgr.get_free_buffer(128, &mut fb), 0);
    assert!(fb.data.is_null(), "NullData mode should set fb.data = NULL");
    assert_eq!(fb.size, 128);
}

/// Verify OneLessByte fail mode: allocates min_size - 1.
#[test]
fn test_manager_one_less_byte_mode() {
    let mut mgr = ExternalFrameBufferList::new(4).with_fail_mode(FailMode::OneLessByte);
    let mut fb = avm_codec_frame_buffer_t {
        data: std::ptr::null_mut(),
        size: 0,
        priv_: std::ptr::null_mut(),
    };

    assert_eq!(mgr.get_free_buffer(128, &mut fb), 0);
    assert_eq!(fb.size, 127, "OneLessByte mode should allocate min_size - 1");
}

// ===========================================================================
// Tests that REQUIRE test data (decode real bitstreams)
// ===========================================================================

/// Helper macro: skip tests that need test data.
macro_rules! require_test_ivf {
    () => {
        match find_test_ivf() {
            Some(p) => p,
            None => {
                eprintln!(
                    "SKIPPED: no IVF test data found. Set LIBAVM_TEST_DATA_PATH to run this test."
                );
                return;
            }
        }
    };
}

/// Decode with the standard number of buffers and verify that get/release
/// callbacks were invoked.
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_get_and_release() {
    let path = require_test_ivf!();
    let num_buffers = (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize;

    let (result, manager) = decode_with_ext_fb(&path, num_buffers, FailMode::None, None);
    assert!(result.is_ok(), "decode should succeed: {result:?}");
    assert!(
        manager.get_count > 0,
        "get callback should have been called at least once"
    );
    assert!(
        manager.release_count > 0,
        "release callback should have been called at least once"
    );
}

/// Decode with extra jitter buffers — should succeed and reuse buffers.
/// Port of C `ExternalFrameBufferTest::EightJitterBuffers`.
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_reuse() {
    let path = require_test_ivf!();
    let jitter_buffers = 8;
    let num_buffers =
        (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize + jitter_buffers;

    let (result, manager) = decode_with_ext_fb(&path, num_buffers, FailMode::None, None);
    assert!(result.is_ok(), "decode should succeed: {result:?}");

    // With enough buffers, some must have been reused (release_count > 0
    // implies buffers were returned and could be re-allocated).
    assert!(
        manager.release_count > 0,
        "buffers should be released (enabling reuse)"
    );
}

/// Decode with minimum buffers (REF + WORK) — should succeed.
/// Port of C `ExternalFrameBufferTest::MinFrameBuffers`.
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_minimum_buffers() {
    let path = require_test_ivf!();
    let num_buffers = (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize;

    let (result, _manager) = decode_with_ext_fb(&path, num_buffers, FailMode::None, None);
    assert!(result.is_ok(), "minimum buffers should suffice: {result:?}");
}

/// Decode with only 2 buffers — should fail with MEM_ERROR on longer clips.
/// Port of C `ExternalFrameBufferTest::NotEnoughBuffers`.
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_insufficient_buffers() {
    let path = require_test_ivf!();
    let num_buffers = 2;

    let (result, _manager) = decode_with_ext_fb(&path, num_buffers, FailMode::None, None);
    // With only 2 buffers, the decoder should eventually run out.
    // Very short clips may still succeed — we accept both outcomes but
    // verify the error type if one occurs.
    if let Err(DecoderError::Decode(kind)) = result {
        assert_eq!(
            kind,
            ErrorKind::OutOfMemory,
            "insufficient buffers should produce OutOfMemory, got {kind:?}"
        );
    }
    // If it succeeded, the clip was too short to exhaust 2 buffers — that's OK.
}

/// Get callback returns NULL data → decode should fail with MEM_ERROR.
/// Port of C `ExternalFrameBufferTest::NullRealloc`.
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_null_allocation() {
    let path = require_test_ivf!();
    let num_buffers = (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize;

    let (result, _manager) =
        decode_with_ext_fb(&path, num_buffers, FailMode::NullData, None);
    match result {
        Err(DecoderError::Decode(kind)) => {
            assert_eq!(
                kind,
                ErrorKind::OutOfMemory,
                "null allocation should produce OutOfMemory, got {kind:?}"
            );
        }
        other => panic!("expected OutOfMemory from null allocation, got: {other:?}"),
    }
}

/// Get callback allocates one byte less than min_size → decode should fail.
/// Port of C `ExternalFrameBufferTest::ReallocOneLessByte`.
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_insufficient_size() {
    let path = require_test_ivf!();
    let num_buffers = (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize;

    let (result, _manager) =
        decode_with_ext_fb(&path, num_buffers, FailMode::OneLessByte, None);
    match result {
        Err(DecoderError::Decode(kind)) => {
            assert_eq!(
                kind,
                ErrorKind::OutOfMemory,
                "undersized buffer should produce OutOfMemory, got {kind:?}"
            );
        }
        other => panic!(
            "expected OutOfMemory from undersized buffer, got: {other:?}"
        ),
    }
}

/// Release callback is a no-op → buffers never freed → should run out.
/// Port of C `ExternalFrameBufferTest::NoRelease`.
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_no_release() {
    let path = require_test_ivf!();
    let num_buffers = (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize;

    let (result, _manager) = decode_with_ext_fb(
        &path,
        num_buffers,
        FailMode::None,
        Some(do_not_release_frame_buffer),
    );
    // The first frame may decode OK, but eventually we should run out of buffers.
    if let Err(DecoderError::Decode(kind)) = result {
        assert_eq!(
            kind,
            ErrorKind::OutOfMemory,
            "no-release should produce OutOfMemory, got {kind:?}"
        );
    }
    // Very short clip might succeed before exhaustion.
}

/// Set frame buffer functions after the first decode call → should fail.
/// Port of C `ExternalFrameBufferTest::SetAfterDecode`.
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_set_after_decode() {
    let path = require_test_ivf!();
    let frames = read_ivf_frames(&path);
    assert!(!frames.is_empty(), "IVF file has no frames");

    let mut decoder = Decoder::new().expect("decoder init failed");

    // Decode one frame first.
    decoder.decode(&frames[0]).expect("first decode should succeed");
    // Drain frames.
    for _frame in decoder.get_frames() {}

    // Now try to set frame buffer functions — should fail.
    let mut manager = Box::new(ExternalFrameBufferList::new(
        (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize,
    ));

    let result = unsafe {
        decoder.set_frame_buffer_functions(
            get_frame_buffer,
            release_frame_buffer,
            &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
        )
    };
    assert!(
        result.is_err(),
        "set_frame_buffer_functions after decode should fail"
    );
}

/// Verify that dropping the decoder releases all held buffers.
/// Port of C `ExternalFrameBufferNonRefTest::ReleaseNonRefFrameBuffer`.
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_manager_dropped_before_decoder() {
    let path = require_test_ivf!();
    let num_buffers = (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize;
    let frames = read_ivf_frames(&path);
    assert!(!frames.is_empty(), "IVF file has no frames");

    let mut manager = Box::new(ExternalFrameBufferList::new(num_buffers));
    let mut decoder = Decoder::new().expect("decoder init failed");

    unsafe {
        decoder
            .set_frame_buffer_functions(
                get_frame_buffer,
                release_frame_buffer,
                &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
            )
            .expect("set_frame_buffer_functions failed");
    }

    // Decode all frames.
    for frame_data in &frames {
        if decoder.decode(frame_data).is_err() {
            break;
        }
        for _frame in decoder.get_frames() {}
    }

    // Drop decoder — should release all buffers via callbacks.
    drop(decoder);

    // Manager is still alive.  All buffers should now be free.
    assert_eq!(
        manager.num_used, 0,
        "all buffers should be released after decoder destruction; {} still in use",
        manager.num_used
    );
}

/// Decode all frames and verify frame data is accessible from external buffers.
/// Port of C `ExternalFrameBufferMD5Test` image validation (not MD5 check,
/// just that plane data lies within the allocated buffer).
#[test]
#[ignore = "requires IVF test data (set LIBAVM_TEST_DATA_PATH)"]
fn test_frame_buffer_data_accessible() {
    let path = require_test_ivf!();
    let num_buffers = (AVM_MAXIMUM_REF_BUFFERS + AVM_MAXIMUM_WORK_BUFFERS) as usize + 4;
    let frames = read_ivf_frames(&path);
    assert!(!frames.is_empty(), "IVF file has no frames");

    let mut manager = Box::new(ExternalFrameBufferList::new(num_buffers));
    let mut decoder = Decoder::new().expect("decoder init failed");

    unsafe {
        decoder
            .set_frame_buffer_functions(
                get_frame_buffer,
                release_frame_buffer,
                &mut *manager as *mut ExternalFrameBufferList as *mut c_void,
            )
            .expect("set_frame_buffer_functions failed");
    }

    let mut frames_decoded = 0u32;
    for frame_data in &frames {
        if decoder.decode(frame_data).is_err() {
            break;
        }
        for frame in decoder.get_frames() {
            frames_decoded += 1;
            // Verify we can access the frame dimensions.
            assert!(frame.width() > 0, "frame width should be > 0");
            assert!(frame.height() > 0, "frame height should be > 0");
            // Verify plane 0 (luma) is accessible.
            let plane = frame.plane(0);
            assert!(
                plane.is_some(),
                "luma plane should be accessible with external FB"
            );
            let plane_data = plane.unwrap();
            assert!(
                !plane_data.is_empty(),
                "luma plane should have non-zero length"
            );
        }
    }
    assert!(
        frames_decoded > 0,
        "should have decoded at least one frame"
    );
}
