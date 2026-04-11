# M0 Walking Skeleton Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build an end-to-end pure-Rust decode path from IVF input to reconstructed `avm_image_t` output that handles a single minimal sub-profile of AV2: one keyframe, one tile, 8-bit 4:2:0, recursive SPLIT partition down to 4×4 leaves, DC intra prediction only, 4×4 DCT_DCT transforms only, fixed QP, no post-filters. The point is to prove the pipeline exists before building tools into it.

**Architecture:** New modules under `src/decoder/` form the pure-Rust core. Every hot-path routine goes through a `Kernels` trait (scalar impl only in M0) and a `TileExecutor` trait (sequential impl only). `backend/rust.rs` becomes a thin shell that delegates to `decoder::core`. Every module outside `decoder/kernels/` forbids `unsafe`. Every kernel and buffer is generic over a `Pixel` type (`u8` only in M0) to prevent the 10-bit retrofit pain flagged as R4 in the spec.

**Tech Stack:** Rust 2021 edition, no new external crates. Existing dev-dep `cargo-miri` for the safe core, existing `fuzz/` crate, existing FFI to libavm (kept as dev-time oracle via `src/diff.rs`).

**Spec reference:** `docs/superpowers/specs/2026-04-11-rustavm-pure-rust-decoder-design.md` §3 M0.

---

## Pre-flight: corpus and oracle strategy

M0 cannot gate on official conformance vectors because no vector matches this sub-profile. The dev-time oracle is **libavm on the same hand-produced stream**: if libavm decodes the stream and writes `avm_image_t` output, the pure-Rust decoder must produce byte-identical output.

**Corpus task (do this before any implementation task):**

- [ ] **Task 0.1: Produce the M0 fixture stream**

**Files:**
- Create: `tests/corpora/m0/README.md`
- Create: `tests/corpora/m0/dc_intra_4x4.ivf` (binary)
- Create: `tests/corpora/m0/dc_intra_4x4.expected.yuv` (binary)

Produce a single-frame IVF file whose only frame is a single-tile keyframe encoded with DC intra and 4×4 transforms at a fixed QP. Two production options, in preference order:

1. **Use the AV2 reference encoder** with the most restrictive knobs available: `--intra-only=1 --tile-columns=0 --tile-rows=0 --tx-size-search=4x4 --max-partition-size=4 --intra-mode=DC --enable-cdef=0 --enable-loop-restoration=0 --enable-film-grain=0` (exact flags discovered during this task — look at the reference encoder's `--help`).
2. **Hand-craft the bytes.** If the encoder can't be constrained this far, write a small Python script under `tests/corpora/m0/mk_fixture.py` that emits the IVF header, OBU headers, sequence header, frame header, and a tile-group payload with coefficients chosen so every 4×4 residual block is zero (pure DC prediction, no residual to decode). This produces the minimal decodable stream: the hand-craft is ~300 lines of bit-packing.

**Validation:**

```bash
cargo run --bin avmdec --features bin -- --backend libavm tests/corpora/m0/dc_intra_4x4.ivf --output tests/corpora/m0/dc_intra_4x4.expected.yuv
```

`dc_intra_4x4.expected.yuv` is then the golden output: the Rust decoder must produce byte-identical YUV.

Commit the README, the IVF, the YUV, and (if used) `mk_fixture.py`.

```bash
git add tests/corpora/m0/
git commit -m "rustavm: add M0 walking-skeleton fixture stream"
```

---

## Phase A — Scaffolding

### Task A.1: Create empty decoder module tree

**Files:**
- Modify: `src/lib.rs` — add `pub mod decoder_core;` line and re-wire `decoder` module.

Actually: the existing `src/decoder.rs` owns the public `Decoder` / `DecoderBuilder` API and must not move. The new internals live under a new sibling module `src/decoder_impl/` to avoid conflicting with the `decoder.rs` file. The target module path from the spec (`decoder::core`, `decoder::entropy`, ...) is achieved by turning `src/decoder.rs` into `src/decoder/mod.rs` and adding submodules alongside it.

- [ ] **Step 1: Convert `src/decoder.rs` to a module directory**

```bash
mkdir -p src/decoder
git mv src/decoder.rs src/decoder/mod.rs
```

- [ ] **Step 2: Add empty submodule files**

Create each of the following with a single `//! <one-line description>` header and nothing else:

- `src/decoder/core.rs` — "Top-level decode loop; drives frame/tile/partition walks."
- `src/decoder/entropy.rs` — "Boolean arithmetic coder and symbol reading."
- `src/decoder/symbols.rs` — "CDF tables and adaptation."
- `src/decoder/partition.rs` — "Superblock partition tree and block-info propagation."
- `src/decoder/intra.rs` — "Intra prediction."
- `src/decoder/transform.rs` — "Inverse transform dispatch (table-driven outer layer)."
- `src/decoder/quant.rs` — "Dequantization and quantization matrices."
- `src/decoder/frame_buffer.rs` — "Reconstructed frame storage; Pixel trait."
- `src/decoder/executor.rs` — "TileExecutor trait; sequential default."

Create `src/decoder/kernels/` directory with:

- `src/decoder/kernels/mod.rs` — "Kernels trait; runtime dispatch."
- `src/decoder/kernels/scalar.rs` — "Portable scalar kernel implementations."

- [ ] **Step 3: Wire the submodules in `src/decoder/mod.rs`**

Add at the top of `src/decoder/mod.rs` (after the existing file docstring):

```rust
pub(crate) mod core;
pub(crate) mod entropy;
pub(crate) mod executor;
pub(crate) mod frame_buffer;
pub(crate) mod intra;
pub(crate) mod kernels;
pub(crate) mod partition;
pub(crate) mod quant;
pub(crate) mod symbols;
pub(crate) mod transform;
```

Modules are `pub(crate)` — the outside world sees the existing `Decoder` / `DecoderBuilder` API only.

- [ ] **Step 4: Build check**

Run: `cargo build -p rustavm`
Expected: builds cleanly with warnings about unused modules (acceptable — they become errors if anything breaks).

- [ ] **Step 5: Commit**

```bash
git add src/decoder/ src/lib.rs
git commit -m "rustavm: scaffold decoder module tree for pure-Rust backend"
```

### Task A.2: Enforce the unsafe policy lint config

**Files:**
- Modify: `src/decoder/mod.rs`
- Modify: each non-kernel submodule created in Task A.1

- [ ] **Step 1: Add unsafe deny at the decoder module root**

In `src/decoder/mod.rs`, add above the module declarations:

```rust
#![deny(unsafe_op_in_unsafe_fn)]
```

- [ ] **Step 2: Forbid unsafe in non-kernel submodules**

Add `#![forbid(unsafe_code)]` as the first line of each of:

- `src/decoder/core.rs`
- `src/decoder/entropy.rs`
- `src/decoder/symbols.rs`
- `src/decoder/partition.rs`
- `src/decoder/intra.rs`
- `src/decoder/transform.rs`
- `src/decoder/quant.rs`
- `src/decoder/executor.rs`

Do **not** add it to `src/decoder/frame_buffer.rs` (FFI raw-pointer interop lives here) or `src/decoder/kernels/*` (SIMD kernels will need `unsafe`).

- [ ] **Step 3: Build check**

Run: `cargo build -p rustavm`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add src/decoder/
git commit -m "rustavm: lock unsafe policy for decoder modules"
```

---

## Phase B — Frame buffer and Pixel trait

### Task B.1: Define the `Pixel` trait

**Files:**
- Modify: `src/decoder/frame_buffer.rs`
- Test: `src/decoder/frame_buffer.rs` (inline `#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing test**

Add to `src/decoder/frame_buffer.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_u8_has_bit_depth_8() {
        assert_eq!(<u8 as Pixel>::BIT_DEPTH, 8);
        assert_eq!(<u8 as Pixel>::MAX, 255);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustavm decoder::frame_buffer::tests::pixel_u8_has_bit_depth_8`
Expected: FAIL — `Pixel` trait not found.

- [ ] **Step 3: Implement the `Pixel` trait**

Add to `src/decoder/frame_buffer.rs`:

```rust
/// Pixel storage type for a reconstructed frame plane.
///
/// M0 implements `u8` only. `u16` lands in M4 when 10-bit support is added.
/// Every buffer and kernel is generic over this trait so the M4 retrofit is
/// monomorphization rather than a rewrite (see spec risk R4).
pub(crate) trait Pixel: Copy + Default + 'static {
    const BIT_DEPTH: u32;
    const MAX: u32;
}

impl Pixel for u8 {
    const BIT_DEPTH: u32 = 8;
    const MAX: u32 = 255;
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p rustavm decoder::frame_buffer::tests::pixel_u8_has_bit_depth_8`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/decoder/frame_buffer.rs
git commit -m "rustavm: add Pixel trait for bit-depth-generic buffers"
```

### Task B.2: Define `FrameBuffer<P>` and `PlaneBuffer<P>`

**Files:**
- Modify: `src/decoder/frame_buffer.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` mod:

```rust
#[test]
fn frame_buffer_allocates_planes_with_aligned_stride() {
    let fb = FrameBuffer::<u8>::new(64, 64, Subsampling::Yuv420);
    assert_eq!(fb.luma().width, 64);
    assert_eq!(fb.luma().height, 64);
    assert_eq!(fb.chroma_u().width, 32);
    assert_eq!(fb.chroma_u().height, 32);
    assert_eq!(fb.luma().stride % 64, 0);
    assert!(fb.luma().stride >= 64);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p rustavm decoder::frame_buffer::tests::frame_buffer_allocates`
Expected: FAIL — missing types.

- [ ] **Step 3: Implement the types**

Add to `src/decoder/frame_buffer.rs`:

```rust
use crate::format::Subsampling;

/// Single plane (Y, U, or V) of a reconstructed frame.
pub(crate) struct PlaneBuffer<P: Pixel> {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    data: Vec<P>,
}

impl<P: Pixel> PlaneBuffer<P> {
    pub fn new(width: usize, height: usize) -> Self {
        let stride = round_up_to(width, 64);
        let data = vec![P::default(); stride * height];
        Self { width, height, stride, data }
    }

    pub fn row(&self, y: usize) -> &[P] {
        &self.data[y * self.stride..y * self.stride + self.width]
    }

    pub fn row_mut(&mut self, y: usize) -> &mut [P] {
        let start = y * self.stride;
        &mut self.data[start..start + self.width]
    }
}

fn round_up_to(n: usize, mult: usize) -> usize {
    (n + mult - 1) / mult * mult
}

/// A reconstructed frame with luma + two chroma planes.
pub(crate) struct FrameBuffer<P: Pixel> {
    luma: PlaneBuffer<P>,
    chroma_u: PlaneBuffer<P>,
    chroma_v: PlaneBuffer<P>,
    subsampling: Subsampling,
}

impl<P: Pixel> FrameBuffer<P> {
    pub fn new(width: usize, height: usize, subsampling: Subsampling) -> Self {
        let (cw, ch) = subsampling.chroma_dims(width, height);
        Self {
            luma: PlaneBuffer::new(width, height),
            chroma_u: PlaneBuffer::new(cw, ch),
            chroma_v: PlaneBuffer::new(cw, ch),
            subsampling,
        }
    }

    pub fn luma(&self) -> &PlaneBuffer<P> { &self.luma }
    pub fn luma_mut(&mut self) -> &mut PlaneBuffer<P> { &mut self.luma }
    pub fn chroma_u(&self) -> &PlaneBuffer<P> { &self.chroma_u }
    pub fn chroma_u_mut(&mut self) -> &mut PlaneBuffer<P> { &mut self.chroma_u }
    pub fn chroma_v(&self) -> &PlaneBuffer<P> { &self.chroma_v }
    pub fn chroma_v_mut(&mut self) -> &mut PlaneBuffer<P> { &mut self.chroma_v }
    pub fn subsampling(&self) -> Subsampling { self.subsampling }
}
```

- [ ] **Step 4: Add `chroma_dims` to `Subsampling` if it's missing**

In `src/format.rs`, add a `chroma_dims` method to `Subsampling` if not present:

```rust
impl Subsampling {
    pub fn chroma_dims(self, w: usize, h: usize) -> (usize, usize) {
        match self {
            Subsampling::Yuv420 => ((w + 1) / 2, (h + 1) / 2),
            Subsampling::Yuv422 => ((w + 1) / 2, h),
            Subsampling::Yuv444 => (w, h),
            Subsampling::Monochrome => (0, 0),
        }
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test -p rustavm decoder::frame_buffer::tests`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/decoder/frame_buffer.rs src/format.rs
git commit -m "rustavm: add FrameBuffer<P>/PlaneBuffer<P> with aligned stride"
```

---

## Phase C — Kernels trait and scalar DCT4×4

### Task C.1: Define the `Kernels` trait with DCT4×4

**Files:**
- Modify: `src/decoder/kernels/mod.rs`
- Modify: `src/decoder/kernels/scalar.rs`

- [ ] **Step 1: Write the failing test**

Create `src/decoder/kernels/mod.rs`:

```rust
//! Kernels trait; runtime dispatch.

pub(crate) mod scalar;

/// Hot-path kernels used by the decoder. SIMD implementations land in M5.
pub(crate) trait Kernels: Sync + 'static {
    /// Inverse 4×4 DCT_DCT. Input is dequantized coefficients in row-major
    /// order; output is residual samples written into `dst` (row-major, stride
    /// in pixels). Coefficient magnitudes fit in i32 per the M0 sub-profile.
    fn inv_dct4x4(&self, coeffs: &[i32; 16], dst: &mut [i16], dst_stride: usize);
}

/// Return the best available kernel implementation for the host CPU.
///
/// M0 returns the scalar impl unconditionally. M5 expands this to runtime
/// CPU-feature dispatch across SIMD backends.
pub(crate) fn detect() -> &'static dyn Kernels {
    &scalar::Scalar
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inv_dct4x4_all_zeros_produces_all_zeros() {
        let k = detect();
        let coeffs = [0i32; 16];
        let mut dst = [7i16; 16];
        k.inv_dct4x4(&coeffs, &mut dst, 4);
        assert_eq!(dst, [0i16; 16]);
    }

    #[test]
    fn inv_dct4x4_dc_only_produces_flat_block() {
        let k = detect();
        let mut coeffs = [0i32; 16];
        coeffs[0] = 64; // DC coefficient scaled to produce value 8 in the 4x4 domain
        let mut dst = [0i16; 16];
        k.inv_dct4x4(&coeffs, &mut dst, 4);
        for &v in dst.iter() {
            assert_eq!(v, dst[0], "DC-only block must be flat");
        }
    }
}
```

- [ ] **Step 2: Run the tests and confirm they fail**

Run: `cargo test -p rustavm decoder::kernels`
Expected: FAIL — `Scalar` not defined.

- [ ] **Step 3: Implement the scalar DCT4×4**

Create `src/decoder/kernels/scalar.rs`:

```rust
//! Portable scalar kernel implementations.

use super::Kernels;

pub(crate) struct Scalar;

impl Kernels for Scalar {
    fn inv_dct4x4(&self, coeffs: &[i32; 16], dst: &mut [i16], dst_stride: usize) {
        // AV2 4x4 DCT_DCT inverse transform. Reference: AV2 spec §7.7.2.1.
        // Port notes:
        //  - 4-point 1D inverse DCT applied to rows, then columns.
        //  - Intermediate values held as i32; final clamp to i16.
        //  - Cosine constants from AV2 §7.7.1 transform cos_bit table.
        // Constants for 4-point inverse DCT (AV2 uses same butterfly as AV1 for 4x4)
        const COS_BIT: i32 = 12;
        const COSPI_16_64: i32 = 11585;
        const COSPI_8_64: i32 = 15137;
        const COSPI_24_64: i32 = 6270;

        let mut tmp = [[0i32; 4]; 4];

        // 1D inverse DCT helper (4-point).
        let idct4 = |input: [i32; 4]| -> [i32; 4] {
            let a0 = input[0];
            let a1 = input[2];
            let a2 = input[1];
            let a3 = input[3];

            let b0 = (a0 + a1) * COSPI_16_64;
            let b1 = (a0 - a1) * COSPI_16_64;
            let b2 = a2 * COSPI_24_64 - a3 * COSPI_8_64;
            let b3 = a2 * COSPI_8_64 + a3 * COSPI_24_64;

            let rnd = 1 << (COS_BIT - 1);
            let c0 = (b0 + rnd) >> COS_BIT;
            let c1 = (b1 + rnd) >> COS_BIT;
            let c2 = (b2 + rnd) >> COS_BIT;
            let c3 = (b3 + rnd) >> COS_BIT;

            [c0 + c3, c1 + c2, c1 - c2, c0 - c3]
        };

        // Row pass.
        for r in 0..4 {
            let row = [
                coeffs[r * 4],
                coeffs[r * 4 + 1],
                coeffs[r * 4 + 2],
                coeffs[r * 4 + 3],
            ];
            tmp[r] = idct4(row);
        }

        // Column pass.
        for c in 0..4 {
            let col = [tmp[0][c], tmp[1][c], tmp[2][c], tmp[3][c]];
            let out = idct4(col);
            for r in 0..4 {
                // Rounding shift for the second stage (AV2 uses 4 for 4x4).
                let v = (out[r] + 8) >> 4;
                dst[r * dst_stride + c] = v.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            }
        }
    }
}
```

> **Note to the engineer implementing this:** the exact cosine constants, rounding shifts, and butterfly structure are from the AV2 spec §7.7. The scalar values above are AV1-family defaults used as a starting point — when the unit tests from Task C.2 against libavm-produced reference blocks fail, update the constants to match the AV2 spec. Do not treat this sketch as authoritative; treat the tests as authoritative.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p rustavm decoder::kernels`
Expected: both tests pass. If `dc_only_produces_flat_block` fails, the rounding/scaling is off — debug against the spec's reference values.

- [ ] **Step 5: Commit**

```bash
git add src/decoder/kernels/
git commit -m "rustavm: add Kernels trait and scalar DCT4x4"
```

### Task C.2: Add a known-answer DCT4×4 test against libavm

**Files:**
- Create: `tests/kat_dct4x4.rs`

- [ ] **Step 1: Extract reference values from libavm**

Write a small test helper (or one-off binary) that calls libavm's inverse DCT4×4 on a few canonical coefficient vectors and prints the output. Commit the resulting values as a test fixture.

Canonical input vectors: `[64, 0, 0, ..., 0]` (DC-only, value 64), `[0, 32, 0, ..., 0]` (AC1), an all-ones impulse, and a random-but-fixed vector. Four test cases is enough.

- [ ] **Step 2: Write the KAT test**

```rust
use rustavm::decoder::kernels::{detect, Kernels};

#[test]
fn kat_dct4x4_dc_only() {
    let k = detect();
    let coeffs = [64i32, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let mut dst = [0i16; 16];
    k.inv_dct4x4(&coeffs, &mut dst, 4);
    // Reference values from libavm's inverse DCT4x4 on the same input.
    let expected = [/* 16 values from libavm reference run */];
    assert_eq!(dst, expected);
}
```

Note: `decoder::kernels` is `pub(crate)`; expose a `pub fn kat_inv_dct4x4` under `#[cfg(feature = "kernel-kat")]` in `src/lib.rs` if needed, or keep this test in `src/decoder/kernels/mod.rs` as an inline test. Prefer inline.

- [ ] **Step 3: Run and iterate**

Run: `cargo test -p rustavm kat_dct4x4`
Expected: PASS once the scalar impl matches libavm. Iterate on the `inv_dct4x4` constants until all four KAT cases pass.

- [ ] **Step 4: Commit**

```bash
git add src/decoder/kernels/ tests/
git commit -m "rustavm: KAT inverse-DCT4x4 against libavm reference values"
```

---

## Phase D — Executor trait

### Task D.1: Sequential `TileExecutor`

**Files:**
- Modify: `src/decoder/executor.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! TileExecutor trait; sequential default.
#![forbid(unsafe_code)]

use std::sync::atomic::{AtomicUsize, Ordering};

/// Dispatches per-tile work to an executor. M0 ships the sequential impl only.
/// M6 adds a threaded impl under the same trait.
pub(crate) trait TileExecutor {
    fn for_each_tile<F>(&self, num_tiles: usize, f: F)
    where
        F: Fn(usize) + Sync;
}

pub(crate) struct Sequential;

impl TileExecutor for Sequential {
    fn for_each_tile<F>(&self, num_tiles: usize, f: F)
    where
        F: Fn(usize) + Sync,
    {
        for i in 0..num_tiles {
            f(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequential_executor_visits_every_tile_in_order() {
        let exec = Sequential;
        let counter = AtomicUsize::new(0);
        let mut seen = vec![0usize; 4];
        // We can't easily check ordering inside `Fn` — instead, use a counter
        // to assert that exactly N invocations happen.
        exec.for_each_tile(4, |i| {
            let n = counter.fetch_add(1, Ordering::SeqCst);
            assert_eq!(n, i);
        });
        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p rustavm decoder::executor`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/decoder/executor.rs
git commit -m "rustavm: add TileExecutor trait with sequential default"
```

---

## Phase E — Boolean arithmetic coder

### Task E.1: BAC reader core

**Files:**
- Modify: `src/decoder/entropy.rs`

The AV2 BAC is the AV1-family boolean arithmetic coder with minor tweaks. The implementation is ~150 lines.

- [ ] **Step 1: Write failing tests for BAC init and `read_bool_unbiased`**

```rust
//! Boolean arithmetic coder and symbol reading.
#![forbid(unsafe_code)]

pub(crate) struct BacReader<'a> {
    buf: &'a [u8],
    pos: usize,
    value: u32,
    range: u32,
    bits_left: i32,
    error: bool,
}

impl<'a> BacReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        let mut r = Self {
            buf,
            pos: 0,
            value: 0,
            range: 0x8000,
            bits_left: -15,
            error: false,
        };
        r.refill();
        r
    }

    pub fn had_error(&self) -> bool { self.error }

    fn refill(&mut self) {
        while self.bits_left < 0 {
            let byte = if self.pos < self.buf.len() {
                let b = self.buf[self.pos];
                self.pos += 1;
                b as u32
            } else {
                self.error = true;
                0
            };
            self.value |= byte << (-self.bits_left as u32 + 8);
            self.bits_left += 8;
        }
    }

    /// Read a single equally-likely bit. Used for raw bits; spec-level
    /// operations use `read_symbol` with a CDF.
    pub fn read_bool_unbiased(&mut self) -> bool {
        self.read_symbol_binary(16384) // p = 0.5 in Q15
    }

    fn read_symbol_binary(&mut self, p: u32) -> bool {
        let split = 1 + (((self.range - 1) * p) >> 15);
        let bigsplit = split << 15;
        let bit;
        if self.value < bigsplit {
            self.range = split;
            bit = false;
        } else {
            self.range -= split;
            self.value -= bigsplit;
            bit = true;
        }
        while self.range < 0x8000 {
            self.range <<= 1;
            self.value <<= 1;
            self.bits_left -= 1;
        }
        self.refill();
        bit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bac_reader_on_empty_buffer_reports_error_after_read() {
        let mut r = BacReader::new(&[]);
        let _ = r.read_bool_unbiased();
        assert!(r.had_error());
    }

    #[test]
    fn bac_reader_decodes_known_sequence() {
        // A byte sequence produced by encoding [true, false, true] with the
        // reference BAC encoder using p=16384 throughout. Update after
        // cross-checking against libavm's encoder output.
        let buf: &[u8] = &[0xa0, 0x00, 0x00, 0x00];
        let mut r = BacReader::new(buf);
        assert_eq!(r.read_bool_unbiased(), true);
        assert_eq!(r.read_bool_unbiased(), false);
        assert_eq!(r.read_bool_unbiased(), true);
        assert!(!r.had_error());
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p rustavm decoder::entropy`
Expected: `bac_reader_decodes_known_sequence` may fail until the fixture bytes are cross-checked against libavm. Iterate: feed the same bit sequence through libavm's BAC encoder, capture the bytes, update the test fixture, re-run.

- [ ] **Step 3: Commit**

```bash
git add src/decoder/entropy.rs
git commit -m "rustavm: add BAC reader core (read_bool_unbiased + read_symbol_binary)"
```

### Task E.2: `read_symbol` against a CDF

**Files:**
- Modify: `src/decoder/entropy.rs`

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn read_symbol_on_two_entry_cdf_matches_binary() {
    // CDF in Q15: [p_0=16384, 32767] means symbol 0 has probability 0.5.
    // On a buffer that deterministically decodes to a sequence of bits,
    // read_symbol should pick the same symbols as read_bool_unbiased.
    let buf: &[u8] = &[0xa0, 0x00, 0x00, 0x00];
    let mut r1 = BacReader::new(buf);
    let s1 = r1.read_symbol(&[16384, 32767]);
    let mut r2 = BacReader::new(buf);
    let s2 = if r2.read_bool_unbiased() { 1 } else { 0 };
    assert_eq!(s1, s2);
}
```

- [ ] **Step 2: Implement `read_symbol`**

Add to `BacReader`:

```rust
/// Read a multi-symbol value against an N-entry CDF in Q15.
/// The CDF is cumulative: `cdf[i]` is P(symbol <= i) * 32768.
/// The last entry must be 32767.
pub fn read_symbol(&mut self, cdf: &[u16]) -> usize {
    let n = cdf.len();
    assert!(n >= 2 && cdf[n - 1] == 32767);
    let mut symbol = 0;
    let mut low: u32 = 0;
    for i in 0..n - 1 {
        let p = cdf[i] as u32 - low;
        let split = 1 + (((self.range - 1 - low_range(self, low)) * p) >> 15);
        let bigsplit = split << 15;
        if self.value < bigsplit {
            self.range = split + low_range(self, low);
            break;
        } else {
            low = cdf[i] as u32;
            symbol = i + 1;
        }
    }
    // Normalize.
    while self.range < 0x8000 {
        self.range <<= 1;
        self.value <<= 1;
        self.bits_left -= 1;
    }
    self.refill();
    symbol
}

fn low_range(_r: &BacReader<'_>, _low: u32) -> u32 { 0 }
```

> **Note:** this sketch is structurally correct but the exact AV2 CDF decode is spec-defined in §4.10.5. Port directly from there — the sketch above is a starting outline only.

- [ ] **Step 3: Run tests**

Run: `cargo test -p rustavm decoder::entropy`
Expected: PASS after matching the spec formulation.

- [ ] **Step 4: Commit**

```bash
git add src/decoder/entropy.rs
git commit -m "rustavm: add BAC read_symbol against CDF"
```

---

## Phase F — Minimal CDF tables and symbol readers

### Task F.1: Symbol reader wrappers for M0's four symbols

M0 only reads four kinds of symbols: `partition` (NONE vs SPLIT, binary), `skip` (forced off in M0 — still read for parser conformance), `intra_mode` (forced DC — read and assert), and the coefficient token subset sufficient for a 4×4 block with typically-zero residuals.

**Files:**
- Modify: `src/decoder/symbols.rs`

- [ ] **Step 1: Define CDF constants**

```rust
//! CDF tables and adaptation.
#![forbid(unsafe_code)]

/// Partition NONE vs SPLIT at a 4×4 leaf. In M0 leaves cannot split further
/// so this CDF is never read at 4×4 — it's read at larger block sizes to
/// force SPLIT all the way down.
pub(crate) const PARTITION_NONE_SPLIT_CDF: [u16; 2] = [
    // [P(NONE), 32767]. Values ported from spec §9.3 for the M0 subprofile's
    // flat CDF init. Replace with real spec values during Task G.3.
    16384, 32767,
];

/// Skip flag. Forced off (=0) in M0; the CDF is still read to stay in sync
/// with the stream.
pub(crate) const SKIP_CDF: [u16; 2] = [29360, 32767];

/// Intra mode selector — M0 only accepts DC (=0). Read and assert.
pub(crate) const INTRA_MODE_CDF: [u16; 13] = [
    // 13 modes, DC first. Flat init until Task G.3 wires the real tables.
    2521, 5042, 7563, 10084, 12605, 15126, 17647, 20168, 22689, 25210, 27731, 30252, 32767,
];
```

- [ ] **Step 2: Reader wrappers in `entropy.rs`**

```rust
use crate::decoder::symbols::*;

impl<'a> BacReader<'a> {
    pub fn read_partition_none_or_split(&mut self) -> PartitionType {
        let s = self.read_symbol(&PARTITION_NONE_SPLIT_CDF);
        if s == 0 { PartitionType::None } else { PartitionType::Split }
    }

    pub fn read_skip(&mut self) -> bool {
        self.read_symbol(&SKIP_CDF) == 1
    }

    pub fn read_intra_mode(&mut self) -> u8 {
        self.read_symbol(&INTRA_MODE_CDF) as u8
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum PartitionType {
    None,
    Split,
}
```

- [ ] **Step 3: Commit**

```bash
git add src/decoder/symbols.rs src/decoder/entropy.rs
git commit -m "rustavm: add M0 symbol readers and flat-init CDFs"
```

---

## Phase G — Bitstream expansion

### Task G.1: Expand `SequenceHeader` to M0-sufficient

**Files:**
- Modify: `src/bitstream.rs`

- [ ] **Step 1: Write failing tests for the new fields**

```rust
#[test]
fn sequence_header_parses_m0_stream() {
    let payload = include_bytes!("../tests/corpora/m0/sh.bin");
    let sh = parse_sequence_header(payload).expect("parse");
    assert_eq!(sh.bit_depth, 8);
    assert_eq!(sh.subsampling_x, 1);
    assert_eq!(sh.subsampling_y, 1);
    assert_eq!(sh.monochrome, false);
}
```

Extract `sh.bin` from the M0 fixture IVF (the bytes of the sequence_header OBU payload) and commit it alongside the fixture.

- [ ] **Step 2: Extend the struct and parser**

Add fields to `SequenceHeader` in `src/bitstream.rs`:

```rust
pub struct SequenceHeader {
    // ... existing fields ...
    pub bit_depth: u8,
    pub monochrome: bool,
    pub subsampling_x: u8,
    pub subsampling_y: u8,
    pub color_range: u8,
    pub chroma_sample_position: u8,
}
```

Extend `parse_sequence_header` to decode the color_config block per AV2 §5.5.2.

- [ ] **Step 3: Run tests**

Run: `cargo test -p rustavm bitstream::tests::sequence_header`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/bitstream.rs tests/corpora/m0/sh.bin
git commit -m "rustavm: parse color_config in sequence header"
```

### Task G.2: `UncompressedFrameHeader` for M0

**Files:**
- Modify: `src/bitstream.rs`

- [ ] **Step 1: Define the struct**

```rust
pub struct UncompressedFrameHeader {
    pub frame_type: FrameType,
    pub show_frame: bool,
    pub frame_size_override_flag: bool,
    pub order_hint: u8,
    pub base_q_idx: u8,
    pub delta_q_y_dc: i8,
    pub delta_q_u_dc: i8,
    pub delta_q_u_ac: i8,
    pub delta_q_v_dc: i8,
    pub delta_q_v_ac: i8,
    pub tx_mode: u8, // 0 = ONLY_4X4 for M0
    pub num_tile_cols: usize,
    pub num_tile_rows: usize,
    pub frame_width: u32,
    pub frame_height: u32,
}
```

- [ ] **Step 2: Implement `parse_uncompressed_frame_header`**

Port §5.9 of the AV2 spec for the KEY frame branch only. Error out on non-KEY frames — M0 streams only have KFs.

- [ ] **Step 3: Write the test against the M0 fixture**

```rust
#[test]
fn uncompressed_frame_header_parses_m0_kf() {
    let payload = include_bytes!("../tests/corpora/m0/fh.bin");
    let sh = /* load fixture SH */;
    let fh = parse_uncompressed_frame_header(&sh, payload).expect("parse");
    assert_eq!(fh.frame_type, FrameType::Key);
    assert_eq!(fh.show_frame, true);
    assert_eq!(fh.num_tile_cols, 1);
    assert_eq!(fh.num_tile_rows, 1);
    assert_eq!(fh.tx_mode, 0);
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rustavm bitstream`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/bitstream.rs tests/corpora/m0/fh.bin
git commit -m "rustavm: parse uncompressed frame header for KF-only M0"
```

### Task G.3: Tile group OBU parser

**Files:**
- Modify: `src/bitstream.rs`

- [ ] **Step 1: Define `TileGroup`**

```rust
pub struct TileGroup<'a> {
    pub tile_start: usize,
    pub tile_end: usize,
    pub data: &'a [u8],
}

pub fn parse_tile_group(sh: &SequenceHeader, fh: &UncompressedFrameHeader, payload: &[u8]) -> Result<TileGroup<'_>, ParseError>;
```

- [ ] **Step 2: Implement per AV2 §5.11.1**

For M0 (single tile), `tile_start == tile_end == 0` and `data` is the full entropy-coded payload.

- [ ] **Step 3: Test**

```rust
#[test]
fn tile_group_single_tile() {
    let sh = /* M0 */; let fh = /* M0 */;
    let payload = include_bytes!("../tests/corpora/m0/tg.bin");
    let tg = parse_tile_group(&sh, &fh, payload).unwrap();
    assert_eq!(tg.tile_start, 0);
    assert_eq!(tg.tile_end, 0);
    assert!(!tg.data.is_empty());
}
```

- [ ] **Step 4: Commit**

```bash
git add src/bitstream.rs tests/corpora/m0/tg.bin
git commit -m "rustavm: parse tile_group OBU"
```

---

## Phase H — Quant and transform dispatch

### Task H.1: Fixed dequant for M0

**Files:**
- Modify: `src/decoder/quant.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Dequantization and quantization matrices.
#![forbid(unsafe_code)]

/// Dequantize a single 4×4 transform block at a fixed QP.
/// M0: no QM, no per-segment deltas, no chroma-specific scaling.
pub(crate) fn dequant_4x4(qindex: u8, coeffs_in: &[i16; 16], coeffs_out: &mut [i32; 16]) {
    let dc_q = dc_q_lookup_8bit(qindex);
    let ac_q = ac_q_lookup_8bit(qindex);
    coeffs_out[0] = coeffs_in[0] as i32 * dc_q as i32;
    for i in 1..16 {
        coeffs_out[i] = coeffs_in[i] as i32 * ac_q as i32;
    }
}

fn dc_q_lookup_8bit(qindex: u8) -> i16 {
    // AV2 spec §7.12.2 dc_qlookup[0] (8-bit path). Port the full 256-entry
    // table during implementation.
    DC_Q_LOOKUP_8[qindex as usize]
}

fn ac_q_lookup_8bit(qindex: u8) -> i16 {
    AC_Q_LOOKUP_8[qindex as usize]
}

const DC_Q_LOOKUP_8: [i16; 256] = [/* port from AV2 spec §7.12.2 */];
const AC_Q_LOOKUP_8: [i16; 256] = [/* port from AV2 spec §7.12.2 */];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dequant_4x4_scales_dc_and_ac_separately() {
        let coeffs_in = [1i16; 16];
        let mut out = [0i32; 16];
        dequant_4x4(10, &coeffs_in, &mut out);
        assert_eq!(out[0], DC_Q_LOOKUP_8[10] as i32);
        assert_eq!(out[1], AC_Q_LOOKUP_8[10] as i32);
    }
}
```

- [ ] **Step 2: Port the quant tables from AV2 spec §7.12.2**

Fill in `DC_Q_LOOKUP_8` and `AC_Q_LOOKUP_8`. These are long but mechanical.

- [ ] **Step 3: Test**

Run: `cargo test -p rustavm decoder::quant`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/decoder/quant.rs
git commit -m "rustavm: add dequant_4x4 with 8-bit Q tables"
```

### Task H.2: Transform dispatch

**Files:**
- Modify: `src/decoder/transform.rs`

- [ ] **Step 1: Implement dispatch**

```rust
//! Inverse transform dispatch (table-driven outer layer).
#![forbid(unsafe_code)]

use crate::decoder::kernels::Kernels;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TxSize {
    Tx4x4,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum TxType {
    DctDct,
}

pub(crate) fn inverse_transform(
    kernels: &dyn Kernels,
    tx_size: TxSize,
    tx_type: TxType,
    coeffs: &[i32; 16],
    dst: &mut [i16],
    stride: usize,
) {
    match (tx_size, tx_type) {
        (TxSize::Tx4x4, TxType::DctDct) => kernels.inv_dct4x4(coeffs, dst, stride),
    }
}
```

- [ ] **Step 2: Inline test**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::kernels::detect;

    #[test]
    fn dispatches_to_dct4x4() {
        let k = detect();
        let coeffs = [0i32; 16];
        let mut dst = [0i16; 16];
        inverse_transform(k, TxSize::Tx4x4, TxType::DctDct, &coeffs, &mut dst, 4);
        assert_eq!(dst, [0i16; 16]);
    }
}
```

- [ ] **Step 3: Commit**

```bash
git add src/decoder/transform.rs
git commit -m "rustavm: add inverse transform dispatch (4x4 DCT_DCT only)"
```

---

## Phase I — DC intra prediction

### Task I.1: `predict_dc_4x4`

**Files:**
- Modify: `src/decoder/intra.rs`

- [ ] **Step 1: Write the failing test**

```rust
//! Intra prediction.
#![forbid(unsafe_code)]

use crate::decoder::frame_buffer::{Pixel, PlaneBuffer};

/// DC intra prediction for a 4×4 block. `above` and `left` are the 4-sample
/// neighbor reference rows/columns; if a neighbor is unavailable pass `None`.
/// Output written into `dst` at row-major stride.
pub(crate) fn predict_dc_4x4<P: Pixel + Into<u32> + TryFrom<u32>>(
    above: Option<&[P; 4]>,
    left: Option<&[P; 4]>,
    dst: &mut [P],
    stride: usize,
) {
    let dc: u32 = match (above, left) {
        (Some(a), Some(l)) => {
            let s: u32 = a.iter().copied().map(Into::into).sum::<u32>()
                + l.iter().copied().map(Into::into).sum::<u32>();
            (s + 4) >> 3
        }
        (Some(a), None) => {
            let s: u32 = a.iter().copied().map(Into::into).sum();
            (s + 2) >> 2
        }
        (None, Some(l)) => {
            let s: u32 = l.iter().copied().map(Into::into).sum();
            (s + 2) >> 2
        }
        (None, None) => 1u32 << (P::BIT_DEPTH - 1),
    };
    let v: P = TryFrom::try_from(dc).unwrap_or_default();
    for r in 0..4 {
        for c in 0..4 {
            dst[r * stride + c] = v;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dc_no_neighbors_produces_mid_gray() {
        let mut dst = [0u8; 16];
        predict_dc_4x4::<u8>(None, None, &mut dst, 4);
        assert_eq!(dst, [128u8; 16]);
    }

    #[test]
    fn dc_with_neighbors_averages_both_sides() {
        let above = [100u8; 4];
        let left = [200u8; 4];
        let mut dst = [0u8; 16];
        predict_dc_4x4::<u8>(Some(&above), Some(&left), &mut dst, 4);
        assert_eq!(dst, [150u8; 16]);
    }
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p rustavm decoder::intra`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add src/decoder/intra.rs
git commit -m "rustavm: add DC intra predictor for 4x4 blocks"
```

---

## Phase J — Partition walk

### Task J.1: Recursive SPLIT walk down to 4×4

**Files:**
- Modify: `src/decoder/partition.rs`

- [ ] **Step 1: Define types**

```rust
//! Superblock partition tree and block-info propagation.
#![forbid(unsafe_code)]

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct BlockSize {
    pub width: usize,
    pub height: usize,
}

impl BlockSize {
    pub const MIN: BlockSize = BlockSize { width: 4, height: 4 };
    pub const SB_M0: BlockSize = BlockSize { width: 64, height: 64 };

    pub fn is_min(self) -> bool { self.width == 4 && self.height == 4 }

    pub fn split(self) -> BlockSize {
        BlockSize { width: self.width / 2, height: self.height / 2 }
    }
}
```

- [ ] **Step 2: Recursive walker with a visitor callback**

```rust
/// Recursively walk a superblock, calling `on_leaf` for every 4×4 leaf block
/// in raster scan order.
pub(crate) fn walk_sb_split_only<F: FnMut(usize, usize, BlockSize)>(
    sb_x: usize,
    sb_y: usize,
    bsize: BlockSize,
    on_leaf: &mut F,
) {
    if bsize.is_min() {
        on_leaf(sb_x, sb_y, bsize);
        return;
    }
    let half_w = bsize.width / 2;
    let half_h = bsize.height / 2;
    let child = bsize.split();
    walk_sb_split_only(sb_x,          sb_y,          child, on_leaf);
    walk_sb_split_only(sb_x + half_w, sb_y,          child, on_leaf);
    walk_sb_split_only(sb_x,          sb_y + half_h, child, on_leaf);
    walk_sb_split_only(sb_x + half_w, sb_y + half_h, child, on_leaf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_64x64_visits_256_4x4_leaves() {
        let mut count = 0;
        walk_sb_split_only(0, 0, BlockSize::SB_M0, &mut |_, _, bs| {
            assert!(bs.is_min());
            count += 1;
        });
        assert_eq!(count, 16 * 16);
    }

    #[test]
    fn walk_visits_leaves_in_z_order() {
        let mut coords = Vec::new();
        walk_sb_split_only(0, 0, BlockSize { width: 8, height: 8 }, &mut |x, y, _| {
            coords.push((x, y));
        });
        assert_eq!(coords, vec![(0, 0), (4, 0), (0, 4), (4, 4)]);
    }
}
```

- [ ] **Step 3: Run**

Run: `cargo test -p rustavm decoder::partition`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/decoder/partition.rs
git commit -m "rustavm: add recursive SPLIT-only partition walker"
```

---

## Phase K — Coefficient reading

### Task K.1: Minimal 4×4 coefficient reader

M0 fixture is crafted so residuals are all zero. The reader must still consume the EOB signal.

**Files:**
- Modify: `src/decoder/entropy.rs`

- [ ] **Step 1: Define `read_coeffs_4x4`**

```rust
/// Read coefficients for a single 4×4 transform block.
/// M0: only handles the all-zero / EOB-at-position-0 case. Future tasks
/// generalize this.
pub fn read_coeffs_4x4(&mut self, out: &mut [i16; 16]) -> Result<(), EntropyError> {
    // In AV2 the all-zero case is signaled by eob == 0.
    // Read the eob flag from the appropriate CDF (see spec §5.11.39).
    let all_zero = self.read_symbol(&ALL_ZERO_CDF) == 1;
    if all_zero {
        *out = [0; 16];
        return Ok(());
    }
    Err(EntropyError::UnimplementedInM0)
}
```

Add `ALL_ZERO_CDF` to `symbols.rs` with a flat init.

- [ ] **Step 2: Test**

```rust
#[test]
fn read_coeffs_4x4_handles_all_zero_case() {
    let buf = /* fixture that encodes all_zero=true */;
    let mut r = BacReader::new(buf);
    let mut coeffs = [1i16; 16];
    r.read_coeffs_4x4(&mut coeffs).unwrap();
    assert_eq!(coeffs, [0i16; 16]);
}
```

- [ ] **Step 3: Commit**

```bash
git add src/decoder/entropy.rs src/decoder/symbols.rs
git commit -m "rustavm: minimal 4x4 coeff reader (all-zero case)"
```

---

## Phase L — Top-level driver

### Task L.1: `decode_tile` function

**Files:**
- Modify: `src/decoder/core.rs`

- [ ] **Step 1: Implement the per-tile loop**

```rust
//! Top-level decode loop; drives frame/tile/partition walks.
#![forbid(unsafe_code)]

use crate::bitstream::{SequenceHeader, UncompressedFrameHeader, TileGroup};
use crate::decoder::entropy::BacReader;
use crate::decoder::frame_buffer::FrameBuffer;
use crate::decoder::intra::predict_dc_4x4;
use crate::decoder::kernels::{detect, Kernels};
use crate::decoder::partition::{walk_sb_split_only, BlockSize};
use crate::decoder::quant::dequant_4x4;
use crate::decoder::transform::{inverse_transform, TxSize, TxType};

pub(crate) fn decode_tile(
    sh: &SequenceHeader,
    fh: &UncompressedFrameHeader,
    tg: &TileGroup<'_>,
    fb: &mut FrameBuffer<u8>,
) -> Result<(), DecodeError> {
    let kernels = detect();
    let mut reader = BacReader::new(tg.data);

    let sb_cols = (fh.frame_width as usize + 63) / 64;
    let sb_rows = (fh.frame_height as usize + 63) / 64;

    for sb_row in 0..sb_rows {
        for sb_col in 0..sb_cols {
            decode_superblock(
                sh, fh, &mut reader, kernels, fb,
                sb_col * 64, sb_row * 64,
            )?;
        }
    }
    Ok(())
}

fn decode_superblock(
    _sh: &SequenceHeader,
    fh: &UncompressedFrameHeader,
    reader: &mut BacReader<'_>,
    kernels: &dyn Kernels,
    fb: &mut FrameBuffer<u8>,
    sb_x: usize,
    sb_y: usize,
) -> Result<(), DecodeError> {
    let mut result = Ok(());
    walk_sb_split_only(sb_x, sb_y, BlockSize::SB_M0, &mut |bx, by, _bs| {
        if result.is_err() { return; }
        if let Err(e) = decode_4x4_block(fh, reader, kernels, fb, bx, by) {
            result = Err(e);
        }
    });
    result
}

fn decode_4x4_block(
    fh: &UncompressedFrameHeader,
    reader: &mut BacReader<'_>,
    kernels: &dyn Kernels,
    fb: &mut FrameBuffer<u8>,
    bx: usize,
    by: usize,
) -> Result<(), DecodeError> {
    // 1. Read intra mode, assert DC.
    let mode = reader.read_intra_mode();
    if mode != 0 {
        return Err(DecodeError::UnexpectedMode);
    }

    // 2. Gather neighbors (or None if unavailable).
    let above = if by >= 4 { Some(gather_above_4(&fb.luma(), bx, by)) } else { None };
    let left = if bx >= 4 { Some(gather_left_4(&fb.luma(), bx, by)) } else { None };

    // 3. Predict into a 4×4 scratch.
    let mut pred = [0u8; 16];
    predict_dc_4x4::<u8>(above.as_ref(), left.as_ref(), &mut pred, 4);

    // 4. Read coefficients (all-zero case only in M0) + dequant + inverse transform.
    let mut coeffs_in = [0i16; 16];
    reader.read_coeffs_4x4(&mut coeffs_in).map_err(|_| DecodeError::EntropyError)?;
    let mut coeffs_out = [0i32; 16];
    dequant_4x4(fh.base_q_idx, &coeffs_in, &mut coeffs_out);
    let mut residual = [0i16; 16];
    inverse_transform(kernels, TxSize::Tx4x4, TxType::DctDct, &coeffs_out, &mut residual, 4);

    // 5. Recon = pred + residual, clamped.
    for r in 0..4 {
        let dst_row = fb.luma_mut().row_mut(by + r);
        for c in 0..4 {
            let p = pred[r * 4 + c] as i32 + residual[r * 4 + c] as i32;
            dst_row[bx + c] = p.clamp(0, 255) as u8;
        }
    }
    Ok(())
}

fn gather_above_4<P: Copy>(plane: &crate::decoder::frame_buffer::PlaneBuffer<P>, bx: usize, by: usize) -> [P; 4] {
    let row = plane.row(by - 1);
    [row[bx], row[bx + 1], row[bx + 2], row[bx + 3]]
}

fn gather_left_4<P: Copy>(plane: &crate::decoder::frame_buffer::PlaneBuffer<P>, bx: usize, by: usize) -> [P; 4] {
    [plane.row(by)[bx - 1], plane.row(by + 1)[bx - 1], plane.row(by + 2)[bx - 1], plane.row(by + 3)[bx - 1]]
}

#[derive(Debug)]
pub(crate) enum DecodeError {
    UnexpectedMode,
    EntropyError,
    Parse(&'static str),
}
```

- [ ] **Step 2: Integration test decoding an all-zero fixture**

Create `tests/m0_walking_skeleton_test.rs`:

```rust
use rustavm::decoder::Decoder;
use rustavm::BackendKind;

#[test]
fn m0_fixture_decodes_bit_exact_vs_libavm() {
    let ivf = include_bytes!("corpora/m0/dc_intra_4x4.ivf");
    let expected = include_bytes!("corpora/m0/dc_intra_4x4.expected.yuv");

    let mut dec = Decoder::builder().backend(BackendKind::Rust).build().unwrap();
    let actual = rustavm::decode_ivf_with_backend(ivf.as_slice(), BackendKind::Rust)
        .expect("rust backend decode");

    assert_eq!(&actual.yuv(), expected, "Rust backend must match libavm byte-exact");
}
```

- [ ] **Step 3: Run and iterate**

Run: `cargo test -p rustavm --test m0_walking_skeleton_test`
Expected: PASS. If it fails, use `src/diff.rs` to localize the mismatch (first frame, first tile, first SB, first block).

- [ ] **Step 4: Commit**

```bash
git add src/decoder/core.rs tests/m0_walking_skeleton_test.rs
git commit -m "rustavm: wire M0 end-to-end decode path; integration test green"
```

---

## Phase M — Wire `backend/rust.rs`

### Task M.1: Delegate to `decoder::core`

**Files:**
- Modify: `src/backend/rust.rs`

- [ ] **Step 1: Replace the `Unimplemented` stub with a call into the new pipeline**

In `src/backend/rust.rs::RustDecoder::decode`, replace the `DecoderError::Unimplemented` branch for frame data with a call through to `crate::decoder::core::decode_tile`. The shim needs to:

1. Keep OBU parsing as-is for sequence header events.
2. When an uncompressed frame header is seen, store it.
3. When a tile group OBU is seen, allocate the `FrameBuffer<u8>` based on SH dimensions and call `decode_tile`.
4. Translate the reconstructed frame to an `avm_image_t` via the existing FFI interop.

Step-by-step code:

```rust
// In RustDecoder::decode after parse_obus succeeds:
for obu in obus {
    let obu_type = ObuType::from_raw(obu.header.obu_type);
    if obu_type == ObuType::TemporalDelimiter { continue; }
    if obu_type == ObuType::SequenceHeader {
        let sh = parse_sequence_header(obu.payload).map_err(map_parse_error)?;
        self.sequence_header = Some(sh);
    }
    if obu_type == ObuType::FrameHeader || obu_type == ObuType::Frame {
        let sh = self.sequence_header.as_ref().ok_or(DecoderError::Parse("frame header before SH"))?;
        let fh = parse_uncompressed_frame_header(sh, obu.payload).map_err(map_parse_error)?;
        self.pending_frame_header = Some(fh);
    }
    if obu_type == ObuType::TileGroup || obu_type == ObuType::Frame {
        let sh = self.sequence_header.as_ref().ok_or(DecoderError::Parse("tile group before SH"))?;
        let fh = self.pending_frame_header.as_ref().ok_or(DecoderError::Parse("tile group before FH"))?;
        let tg = parse_tile_group(sh, fh, obu.payload).map_err(map_parse_error)?;
        let mut fb = FrameBuffer::<u8>::new(fh.frame_width as usize, fh.frame_height as usize, Subsampling::Yuv420);
        crate::decoder::core::decode_tile(sh, fh, &tg, &mut fb).map_err(map_decode_error)?;
        self.pending_output = Some(fb);
    }
}
```

- [ ] **Step 2: Implement `get_frame` to produce `avm_image_t`**

The Rust backend's `get_frame` currently returns `None`. Implement it to take `self.pending_output`, convert it into an `avm_image_t`, and return `NonNull<avm_image_t>`. The conversion uses the existing FFI types; it needs `unsafe` for the raw-pointer interop (allowed per §4.3 of the spec).

Because `avm_image_t` expects C-owned memory and we own the buffer in Rust, the shim holds onto the `FrameBuffer` inside the decoder so the pointers stay valid until the next `decode` call.

- [ ] **Step 3: Run the M0 integration test**

Run: `cargo test -p rustavm --test m0_walking_skeleton_test`
Expected: PASS end-to-end.

- [ ] **Step 4: Commit**

```bash
git add src/backend/rust.rs
git commit -m "rustavm: wire Rust backend to pure-Rust decode path for M0"
```

---

## Phase N — Final checks

### Task N.1: CI wiring

**Files:**
- Modify: `.github/workflows/*.yml` (if CI exists) or `Cargo.toml`

- [ ] **Step 1: Add Miri to the test matrix**

Run the whole test suite except `tests/kat_dct4x4.rs` under Miri in CI. Miri can't handle FFI calls into libavm, so the M0 integration test is excluded — Miri runs only on the pure-Rust modules.

- [ ] **Step 2: Add `clippy::undocumented_unsafe_blocks` to clippy config**

- [ ] **Step 3: Commit**

```bash
git add .github .clippy.toml
git commit -m "rustavm: add M0 CI lint and Miri coverage"
```

### Task N.2: M0 exit checklist

- [ ] `cargo test -p rustavm` passes clean.
- [ ] `cargo test -p rustavm --test m0_walking_skeleton_test` passes (bit-exact vs libavm on the M0 fixture).
- [ ] `cargo miri test -p rustavm` passes on the scalar core.
- [ ] `cargo clippy -p rustavm -- -D warnings` clean.
- [ ] `backend::Rust` decodes the M0 fixture through the full public `Decoder` API.
- [ ] No `TODO`, `FIXME`, or `unimplemented!()` in `src/decoder/`.

When all boxes are checked, merge the M0 branch and start on M1.
