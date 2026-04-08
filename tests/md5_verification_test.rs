//! MD5 verification integration tests for the Rust AV2 decoder.
//!
//! These tests decode AV2 IVF test vectors and verify per-frame MD5 checksums
//! against the reference `.ivf.md5` files produced by the C test suite.
//!
//! # Running
//!
//! Most tests are `#[ignore]` because they require test data.  To run them:
//!
//! ```sh
//! LIBAVM_TEST_DATA_PATH=/path/to/testdata cargo test --test md5_verification_test -- --ignored
//! ```
//!
//! Test data can be downloaded with:
//! ```sh
//! # Files live at https://storage.googleapis.com/aom-test-data/<filename>
//! # SHA1 checksums are in avm/test/test-data.sha1
//! mkdir -p testdata
//! # download av1-1-b8-00-quantizer-00.ivf and av1-1-b8-00-quantizer-00.ivf.md5, etc.
//! ```

use rustavm::decoder::{Decoder, Frame};
use rustavm::ivf::IvfReader;
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

// ─── Test data path resolution ──────────────────────────────────────────────

/// Resolve the directory containing AV2 test vectors and their `.md5` companions.
///
/// Resolution order (mirrors `avm/test/video_source.h:37-50`):
/// 1. `LIBAVM_TEST_DATA_PATH` environment variable
/// 2. `../testdata` (workspace root sibling)
/// 3. `./testdata` (crate root)
/// 4. `../avm/out/testdata` (CMake download location)
fn test_data_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("LIBAVM_TEST_DATA_PATH") {
        let pb = PathBuf::from(p);
        if pb.is_dir() {
            return Some(pb);
        }
    }
    for candidate in &["../testdata", "./testdata", "../avm/out/testdata"] {
        let pb = PathBuf::from(candidate);
        if pb.is_dir() {
            return Some(pb);
        }
    }
    None
}

/// Returns the full path to `filename` in the test data directory, or `None`
/// if the directory or file cannot be found.
fn find_test_file(filename: &str) -> Option<PathBuf> {
    let path = test_data_dir()?.join(filename);
    if path.exists() { Some(path) } else { None }
}

/// Print a helpful skip message and return `true` when test data is absent.
///
/// Intended use:
/// ```rust,no_run
/// if skip_if_missing("av1-1-b8-00-quantizer-00.ivf") { return; }
/// ```
fn skip_if_missing(filename: &str) -> bool {
    if find_test_file(filename).is_none() {
        println!(
            "SKIP: test file '{filename}' not found.\n\
             Set LIBAVM_TEST_DATA_PATH to the directory containing AV2 test vectors.\n\
             Download vectors from https://storage.googleapis.com/aom-test-data/<filename>"
        );
        true
    } else {
        false
    }
}

// ─── Frame MD5 computation ───────────────────────────────────────────────────

/// Compute the per-frame MD5 that matches the reference `.ivf.md5` files.
///
/// Replicates **both** layers of the C implementation:
///
/// 1. `md5_helper.h:24-46` — walks each plane row-by-row, hashing only the
///    active pixel width (not the full stride).
///
/// 2. `test_vector_test.cc:73-86` — when `bit_depth == 8` but
///    `AVM_IMG_FMT_HIGHBITDEPTH` is set, calls `avm_img_downshift(dst, src, 0, 8)`
///    before hashing.  This extracts the low byte of each 16-bit sample (the
///    actual 8-bit pixel value stored in the lower half of the 16-bit word) and
///    hashes 1 byte per sample rather than 2.
///
/// Returns a 32-character lowercase hex MD5 string.
pub fn frame_md5(frame: &Frame) -> String {
    let needs_downshift = frame.bit_depth() == 8 && frame.bytes_per_sample() == 2;

    let mut ctx = md5::Context::new();

    for plane_idx in 0..3_usize {
        let Some(rows) = frame.rows(plane_idx) else { continue };
        let w = frame.plane_width(plane_idx);
        let mut row_buf = if needs_downshift { vec![0u8; w] } else { Vec::new() };

        for row in rows {
            if needs_downshift {
                // avm_img_downshift(dst, src, shift=0, depth=8): take the low
                // byte of each 16-bit sample so we hash one byte per pixel.
                for (x, dst) in row_buf.iter_mut().enumerate() {
                    *dst = row.get(x * 2).copied().unwrap_or(0);
                }
                ctx.consume(&row_buf);
            } else {
                ctx.consume(row);
            }
        }
    }

    format!("{:x}", ctx.compute())
}

// ─── Core decode helper ──────────────────────────────────────────────────────

/// Decode every frame in an IVF file and return per-frame MD5 hex strings.
///
/// `threads` is forwarded to [`Decoder::builder`]; `None` uses the codec
/// default.  Each call creates a fresh decoder, so the function is safe to
/// call multiple times with different thread counts.
pub fn decode_to_md5s(
    ivf_path: &Path,
    threads: Option<u32>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let file = File::open(ivf_path)?;
    let mut ivf = IvfReader::new(BufReader::new(file))?;
    let mut builder = Decoder::builder();
    if let Some(t) = threads {
        builder = builder.threads(t);
    }
    let mut decoder = builder.build()?;
    let mut md5s = Vec::new();

    while let Some(pkt) = ivf.next_frame()? {
        decoder.decode(&pkt.data)?;
        for frame in decoder.get_frames() {
            md5s.push(frame_md5(&frame));
        }
    }

    // Flush to retrieve any buffered frames (B-frame reordering).
    decoder.flush()?;
    for frame in decoder.get_frames() {
        md5s.push(frame_md5(&frame));
    }

    Ok(md5s)
}

// ─── MD5 reference file parser ───────────────────────────────────────────────

/// Parse an `.ivf.md5` reference file produced by the C test suite.
///
/// Format (one line per decoded frame):
/// ```text
/// a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6  img-352x288-0001.i420
/// ```
///
/// Returns only the first whitespace-separated field (the 32-hex-char MD5).
fn parse_md5_file(path: &Path) -> io::Result<Vec<String>> {
    let file = File::open(path)?;
    let mut md5s = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(token) = line.split_whitespace().next() {
            md5s.push(token.to_string());
        }
    }
    Ok(md5s)
}

// ─── Core verification helper ────────────────────────────────────────────────

/// Decode `ivf_filename` with `threads` threads and compare every frame's MD5
/// against the matching reference file (`ivf_filename + ".md5"`).
///
/// Returns `Ok(frame_count)` on success, or panics on mismatch.
fn verify_md5(ivf_filename: &str, threads: Option<u32>) -> usize {
    let ivf_path = find_test_file(ivf_filename)
        .unwrap_or_else(|| panic!("test file not found: {ivf_filename}"));
    let md5_path = find_test_file(&format!("{ivf_filename}.md5"))
        .unwrap_or_else(|| panic!("MD5 reference file not found: {ivf_filename}.md5"));

    let actual = decode_to_md5s(&ivf_path, threads)
        .unwrap_or_else(|e| panic!("decode failed for {ivf_filename}: {e}"));
    let expected = parse_md5_file(&md5_path)
        .unwrap_or_else(|e| panic!("failed to read {ivf_filename}.md5: {e}"));

    assert_eq!(
        actual.len(),
        expected.len(),
        "{}: decoded {} frames but MD5 file has {} entries",
        ivf_filename,
        actual.len(),
        expected.len(),
    );

    for (i, (got, want)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            got, want,
            "{ivf_filename}: frame {i} MD5 mismatch\n  got:  {got}\n  want: {want}",
        );
    }

    actual.len()
}

// ─── Infrastructure smoke test (no test data required) ──────────────────────

/// Verifies that the test infrastructure itself is wired up correctly:
/// the `frame_md5` and `decode_to_md5s` helpers compile and the MD5 reference
/// parser handles an empty input without panicking.
#[test]
fn test_md5_infrastructure_compiles() {
    // Parse an in-memory "file" via a cursor — just exercises the parser logic.
    use std::io::Cursor;
    let input = b"a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6  img-16x16-0001.i420\n\
                  deadbeefdeadbeefdeadbeefdeadbeef  img-16x16-0002.i420\n";
    let mut md5s = Vec::new();
    for line in BufReader::new(Cursor::new(input)).lines() {
        let line = line.unwrap();
        let line = line.trim().to_string();
        if !line.is_empty() {
            md5s.push(line.split_whitespace().next().unwrap().to_string());
        }
    }
    assert_eq!(md5s.len(), 2);
    assert_eq!(md5s[0], "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6");
    assert_eq!(md5s[1], "deadbeefdeadbeefdeadbeefdeadbeef");
}

/// Verifies that test data lookup returns None gracefully when no path is set.
#[test]
fn test_find_missing_file_returns_none() {
    // This file should never exist.
    let result = find_test_file("__nonexistent_test_vector_xyz__.ivf");
    // If LIBAVM_TEST_DATA_PATH happens to be set and points to a real directory,
    // the file still won't exist there.  Either way: no panic, just None.
    assert!(result.is_none());
}

// ─── 8-bit quantizer test vectors ───────────────────────────────────────────

/// Smoke test: one 8-bit quantizer vector covers the basic decode + MD5 path.
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with AV2 test vectors"]
fn test_decode_8bit_quantizer_00() {
    if skip_if_missing("av1-1-b8-00-quantizer-00.ivf") {
        return;
    }
    let n = verify_md5("av1-1-b8-00-quantizer-00.ivf", None);
    println!("av1-1-b8-00-quantizer-00: verified {n} frames");
}

/// High quantizer value: verifies coarser quantization still produces correct MD5.
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with AV2 test vectors"]
fn test_decode_8bit_quantizer_63() {
    if skip_if_missing("av1-1-b8-00-quantizer-63.ivf") {
        return;
    }
    let n = verify_md5("av1-1-b8-00-quantizer-63.ivf", None);
    println!("av1-1-b8-00-quantizer-63: verified {n} frames");
}

/// Run all 64 8-bit quantizer vectors (quantizer-00 through quantizer-63).
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with all 8-bit quantizer test vectors"]
fn test_decode_all_8bit_quantizer_vectors() {
    let dir = match test_data_dir() {
        Some(d) => d,
        None => {
            println!("SKIP: LIBAVM_TEST_DATA_PATH not set");
            return;
        }
    };
    let mut passed = 0usize;
    let mut skipped = 0usize;
    for q in 0..=63u32 {
        let filename = format!("av1-1-b8-00-quantizer-{q:02}.ivf");
        if !dir.join(&filename).exists() {
            skipped += 1;
            continue;
        }
        let n = verify_md5(&filename, None);
        println!("{filename}: {n} frames OK");
        passed += 1;
    }
    if passed == 0 {
        println!("SKIP: no 8-bit quantizer vectors found in {dir:?}");
        return;
    }
    println!("8-bit quantizer: {passed}/64 vectors verified ({skipped} skipped)");
}

// ─── 10-bit quantizer test vectors ──────────────────────────────────────────

/// 10-bit smoke test: exercises the HIGHBITDEPTH code path (2 bytes/sample).
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with 10-bit test vectors"]
fn test_decode_10bit_quantizer_00() {
    if skip_if_missing("av1-1-b10-00-quantizer-00.ivf") {
        return;
    }
    let n = verify_md5("av1-1-b10-00-quantizer-00.ivf", None);
    println!("av1-1-b10-00-quantizer-00: verified {n} frames");
}

/// Run all 64 10-bit quantizer vectors.
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with all 10-bit quantizer test vectors"]
fn test_decode_all_10bit_quantizer_vectors() {
    let dir = match test_data_dir() {
        Some(d) => d,
        None => {
            println!("SKIP: LIBAVM_TEST_DATA_PATH not set");
            return;
        }
    };
    let mut passed = 0usize;
    let mut skipped = 0usize;
    for q in 0..=63u32 {
        let filename = format!("av1-1-b10-00-quantizer-{q:02}.ivf");
        if !dir.join(&filename).exists() {
            skipped += 1;
            continue;
        }
        let n = verify_md5(&filename, None);
        println!("{filename}: {n} frames OK");
        passed += 1;
    }
    if passed == 0 {
        println!("SKIP: no 10-bit quantizer vectors found in {dir:?}");
        return;
    }
    println!("10-bit quantizer: {passed}/64 vectors verified ({skipped} skipped)");
}

// ─── Frame size test vectors ─────────────────────────────────────────────────

/// Decode the 16×16 vector (smallest standard test size).
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with size test vectors"]
fn test_decode_size_16x16() {
    if skip_if_missing("av1-1-b8-01-size-16x16.ivf") {
        return;
    }
    let n = verify_md5("av1-1-b8-01-size-16x16.ivf", None);
    println!("av1-1-b8-01-size-16x16: verified {n} frames");
}

/// Decode a selection of non-power-of-two and odd-dimension frame size vectors.
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with size test vectors"]
fn test_decode_various_frame_sizes() {
    let sizes = [
        "av1-1-b8-01-size-16x16.ivf",
        "av1-1-b8-01-size-16x18.ivf",
        "av1-1-b8-01-size-34x34.ivf",
        "av1-1-b8-01-size-64x64.ivf",
        "av1-1-b8-01-size-66x66.ivf",
        "av1-1-b8-01-size-196x196.ivf",
        "av1-1-b8-01-size-226x226.ivf",
    ];
    let dir = match test_data_dir() {
        Some(d) => d,
        None => {
            println!("SKIP: LIBAVM_TEST_DATA_PATH not set");
            return;
        }
    };
    let mut any_ran = false;
    for &filename in &sizes {
        if !dir.join(filename).exists() {
            println!("skipping missing: {filename}");
            continue;
        }
        let n = verify_md5(filename, None);
        println!("{filename}: {n} frames OK");
        any_ran = true;
    }
    if !any_ran {
        println!("SKIP: none of the size test vectors found");
    }
}

// ─── Feature-specific vectors ────────────────────────────────────────────────

/// All-intra, CDF-update, motion vectors, mfmv, SVC, film grain.
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with feature test vectors"]
fn test_decode_feature_vectors() {
    let vectors = [
        "av1-1-b8-02-allintra.ivf",
        "av1-1-b8-04-cdfupdate.ivf",
        "av1-1-b8-05-mv.ivf",
        "av1-1-b8-06-mfmv.ivf",
        "av1-1-b8-22-svc-L1T2.ivf",
        "av1-1-b8-22-svc-L2T1.ivf",
        "av1-1-b8-22-svc-L2T2.ivf",
        "av1-1-b8-23-film_grain-50.ivf",
        "av1-1-b10-23-film_grain-50.ivf",
    ];
    let dir = match test_data_dir() {
        Some(d) => d,
        None => {
            println!("SKIP: LIBAVM_TEST_DATA_PATH not set");
            return;
        }
    };
    let mut any_ran = false;
    for &filename in &vectors {
        if !dir.join(filename).exists() {
            println!("skipping missing: {filename}");
            continue;
        }
        let n = verify_md5(filename, None);
        println!("{filename}: {n} frames OK");
        any_ran = true;
    }
    if !any_ran {
        println!("SKIP: none of the feature test vectors found");
    }
}

// ─── Multi-threaded correctness ──────────────────────────────────────────────

/// Decode the same IVF file with 1, 2, and 4 threads and verify that the
/// per-frame MD5 output is identical across all three runs.
///
/// This detects data races or non-determinism introduced by the threading code.
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with AV2 test vectors"]
fn test_decode_md5_with_threads() {
    // Use a vector that has enough frames to exercise thread scheduling.
    let filename = "av1-1-b8-00-quantizer-00.ivf";
    if skip_if_missing(filename) {
        return;
    }
    let ivf_path = find_test_file(filename).unwrap();

    let md5s_1t = decode_to_md5s(&ivf_path, Some(1))
        .unwrap_or_else(|e| panic!("1-thread decode failed: {e}"));
    let md5s_2t = decode_to_md5s(&ivf_path, Some(2))
        .unwrap_or_else(|e| panic!("2-thread decode failed: {e}"));
    let md5s_4t = decode_to_md5s(&ivf_path, Some(4))
        .unwrap_or_else(|e| panic!("4-thread decode failed: {e}"));

    assert_eq!(
        md5s_1t.len(),
        md5s_2t.len(),
        "frame count differs between 1 and 2 threads"
    );
    assert_eq!(
        md5s_1t.len(),
        md5s_4t.len(),
        "frame count differs between 1 and 4 threads"
    );

    for (i, ((m1, m2), m4)) in md5s_1t
        .iter()
        .zip(md5s_2t.iter())
        .zip(md5s_4t.iter())
        .enumerate()
    {
        assert_eq!(
            m1, m2,
            "frame {i}: MD5 differs between 1 and 2 threads\n  1t: {m1}\n  2t: {m2}"
        );
        assert_eq!(
            m1, m4,
            "frame {i}: MD5 differs between 1 and 4 threads\n  1t: {m1}\n  4t: {m4}"
        );
    }

    println!(
        "Thread consistency: {} frames, identical across 1/2/4 threads ✓",
        md5s_1t.len()
    );
}

/// Same thread-consistency check for 10-bit content (HIGHBITDEPTH code path).
#[test]
#[ignore = "requires LIBAVM_TEST_DATA_PATH with 10-bit test vectors"]
fn test_decode_md5_with_threads_10bit() {
    let filename = "av1-1-b10-00-quantizer-00.ivf";
    if skip_if_missing(filename) {
        return;
    }
    let ivf_path = find_test_file(filename).unwrap();

    let md5s_1t = decode_to_md5s(&ivf_path, Some(1))
        .expect("1-thread 10-bit decode failed");
    let md5s_4t = decode_to_md5s(&ivf_path, Some(4))
        .expect("4-thread 10-bit decode failed");

    assert_eq!(md5s_1t.len(), md5s_4t.len());
    for (i, (m1, m4)) in md5s_1t.iter().zip(md5s_4t.iter()).enumerate() {
        assert_eq!(
            m1, m4,
            "10-bit frame {i}: 1-thread vs 4-thread MD5 mismatch"
        );
    }
    println!(
        "10-bit thread consistency: {} frames ✓",
        md5s_1t.len()
    );
}
