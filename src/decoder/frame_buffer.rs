//! Reconstructed frame storage; Pixel trait.

use crate::format::Subsampling;

/// Pixel storage type for a reconstructed frame plane.
///
/// M0 implements `u8` only. `u16` lands in M4 when 10-bit support is added.
/// Every buffer and kernel is generic over this trait so the M4 retrofit is
/// monomorphization rather than a rewrite.
pub(crate) trait Pixel: Copy + Default + 'static {
    const BIT_DEPTH: u32;
    const MAX: u32;
}

impl Pixel for u8 {
    const BIT_DEPTH: u32 = 8;
    const MAX: u32 = 255;
}

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
        Self {
            width,
            height,
            stride,
            data,
        }
    }

    pub fn row(&self, y: usize) -> &[P] {
        let start = y * self.stride;
        &self.data[start..start + self.width]
    }

    pub fn row_mut(&mut self, y: usize) -> &mut [P] {
        let start = y * self.stride;
        &mut self.data[start..start + self.width]
    }

    pub fn data(&self) -> &[P] {
        &self.data
    }
}

fn round_up_to(n: usize, mult: usize) -> usize {
    n.div_ceil(mult) * mult
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

    pub fn luma(&self) -> &PlaneBuffer<P> {
        &self.luma
    }

    pub fn luma_mut(&mut self) -> &mut PlaneBuffer<P> {
        &mut self.luma
    }

    pub fn chroma_u(&self) -> &PlaneBuffer<P> {
        &self.chroma_u
    }

    pub fn chroma_u_mut(&mut self) -> &mut PlaneBuffer<P> {
        &mut self.chroma_u
    }

    pub fn chroma_v(&self) -> &PlaneBuffer<P> {
        &self.chroma_v
    }

    pub fn chroma_v_mut(&mut self) -> &mut PlaneBuffer<P> {
        &mut self.chroma_v
    }

    pub fn subsampling(&self) -> Subsampling {
        self.subsampling
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_u8_has_bit_depth_8() {
        assert_eq!(<u8 as Pixel>::BIT_DEPTH, 8);
        assert_eq!(<u8 as Pixel>::MAX, 255);
    }

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
}
