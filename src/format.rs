//! Type-safe wrappers around libavm pixel format and colorspace constants.
//!
//! These enums replace the bindgen-generated `avm_img_fmt`,
//! `avm_chroma_sample_position`, and `avm_color_range` integer types in the
//! public API of [`Frame`](crate::decoder::Frame).  The raw constants are
//! still available under [`crate::ffi`] for advanced use.

use crate::sys;

/// Typed view over a single decoded image plane.
///
/// 8-bit formats yield [`PlaneView::U8`]; high-bit-depth (10/12-bit) formats
/// stored in 16-bit containers yield [`PlaneView::U16`].  In both variants
/// the slice covers the **full `stride * height` plane storage**, including
/// any per-row padding past the active pixel width.  Use
/// [`Frame::rows`](crate::decoder::Frame::rows) for an iterator that already
/// crops to the active row width.
#[derive(Debug)]
pub enum PlaneView<'a> {
    /// Plane is 8-bit; one byte per sample.
    U8(&'a [u8]),
    /// Plane is high-bit-depth in a 16-bit container; one `u16` per sample.
    U16(&'a [u16]),
}

impl<'a> PlaneView<'a> {
    /// Returns the number of samples (pixels) in the plane storage,
    /// including any per-row padding from stride > active row width.
    pub fn len(&self) -> usize {
        match self {
            Self::U8(s) => s.len(),
            Self::U16(s) => s.len(),
        }
    }

    /// Returns `true` if the plane storage is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Chroma subsampling pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Subsampling {
    /// 4:2:0 — chroma at half horizontal and half vertical resolution.
    Yuv420,
    /// 4:2:2 — chroma at half horizontal resolution.
    Yuv422,
    /// 4:4:4 — full-resolution chroma.
    Yuv444,
}

impl Subsampling {
    /// Return the chroma plane dimensions for a luma frame size.
    pub fn chroma_dims(self, w: usize, h: usize) -> (usize, usize) {
        match self {
            Self::Yuv420 => (w.div_ceil(2), h.div_ceil(2)),
            Self::Yuv422 => (w.div_ceil(2), h),
            Self::Yuv444 => (w, h),
        }
    }
}

/// Pixel format of a decoded frame.
///
/// Combines chroma subsampling with the storage container width (8-bit
/// packed vs. 16-bit container for high-bit-depth content).  The 16-bit
/// container variants are used for 10/12-bit content stored two bytes per
/// sample; query [`PixelFormat::is_high_bit_depth`] to distinguish.
///
/// `#[non_exhaustive]` because libavm may add new pixel formats in future
/// versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PixelFormat {
    /// 8-bit YUV 4:2:0 (libavm `AVM_IMG_FMT_I420`).
    I420,
    /// 8-bit YUV 4:2:2 (libavm `AVM_IMG_FMT_I422`).
    I422,
    /// 8-bit YUV 4:4:4 (libavm `AVM_IMG_FMT_I444`).
    I444,
    /// 8-bit YV12 — planar 4:2:0 with V plane before U.
    Yv12,
    /// 16-bit-container YUV 4:2:0 (libavm `AVM_IMG_FMT_I42016`).
    I42016,
    /// 16-bit-container YUV 4:2:2 (libavm `AVM_IMG_FMT_I42216`).
    I42216,
    /// 16-bit-container YUV 4:4:4 (libavm `AVM_IMG_FMT_I44416`).
    I44416,
    /// 16-bit-container YV12.
    Yv1216,
}

impl PixelFormat {
    /// Convert from the raw libavm `avm_img_fmt` integer.
    ///
    /// Returns `None` if the value is not one of the recognized variants.
    pub fn from_raw(fmt: sys::avm_img_fmt) -> Option<Self> {
        match fmt {
            v if v == sys::avm_img_fmt_AVM_IMG_FMT_I420 => Some(Self::I420),
            v if v == sys::avm_img_fmt_AVM_IMG_FMT_I422 => Some(Self::I422),
            v if v == sys::avm_img_fmt_AVM_IMG_FMT_I444 => Some(Self::I444),
            v if v == sys::avm_img_fmt_AVM_IMG_FMT_YV12 => Some(Self::Yv12),
            v if v == sys::avm_img_fmt_AVM_IMG_FMT_I42016 => Some(Self::I42016),
            v if v == sys::avm_img_fmt_AVM_IMG_FMT_I42216 => Some(Self::I42216),
            v if v == sys::avm_img_fmt_AVM_IMG_FMT_I44416 => Some(Self::I44416),
            v if v == sys::avm_img_fmt_AVM_IMG_FMT_YV1216 => Some(Self::Yv1216),
            _ => None,
        }
    }

    /// Returns the raw libavm `avm_img_fmt` integer for this format.
    pub fn as_raw(self) -> sys::avm_img_fmt {
        match self {
            Self::I420 => sys::avm_img_fmt_AVM_IMG_FMT_I420,
            Self::I422 => sys::avm_img_fmt_AVM_IMG_FMT_I422,
            Self::I444 => sys::avm_img_fmt_AVM_IMG_FMT_I444,
            Self::Yv12 => sys::avm_img_fmt_AVM_IMG_FMT_YV12,
            Self::I42016 => sys::avm_img_fmt_AVM_IMG_FMT_I42016,
            Self::I42216 => sys::avm_img_fmt_AVM_IMG_FMT_I42216,
            Self::I44416 => sys::avm_img_fmt_AVM_IMG_FMT_I44416,
            Self::Yv1216 => sys::avm_img_fmt_AVM_IMG_FMT_YV1216,
        }
    }

    /// Returns `true` if this format uses 16-bit storage per sample.
    ///
    /// 10-bit and 12-bit content is stored in 16-bit containers; the
    /// `AVM_IMG_FMT_HIGHBITDEPTH` flag bit is set on the underlying
    /// `avm_img_fmt` integer.
    pub fn is_high_bit_depth(self) -> bool {
        self.as_raw() & sys::AVM_IMG_FMT_HIGHBITDEPTH != 0
    }

    /// Returns the chroma subsampling pattern of this format.
    pub fn subsampling(self) -> Subsampling {
        match self {
            Self::I420 | Self::I42016 | Self::Yv12 | Self::Yv1216 => Subsampling::Yuv420,
            Self::I422 | Self::I42216 => Subsampling::Yuv422,
            Self::I444 | Self::I44416 => Subsampling::Yuv444,
        }
    }

    /// Returns the number of bytes per sample (1 for 8-bit, 2 for high
    /// bit depth).
    pub fn bytes_per_sample(self) -> usize {
        if self.is_high_bit_depth() { 2 } else { 1 }
    }
}

/// Position of chroma samples relative to the luma grid.
///
/// Mirrors libavm's `avm_chroma_sample_position` enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChromaSamplePosition {
    /// Horizontally co-sited with luma column 0, vertically between rows.
    /// MPEG-2 / "left" position (libavm `AVM_CSP_LEFT`).
    Left,
    /// Centered horizontally and vertically between luma samples
    /// (libavm `AVM_CSP_CENTER`).
    Center,
    /// Co-located with the top-left luma sample
    /// (libavm `AVM_CSP_TOPLEFT`).
    TopLeft,
    /// Top-centered (libavm `AVM_CSP_TOP`).
    Top,
    /// Bottom-left (libavm `AVM_CSP_BOTTOMLEFT`).
    BottomLeft,
    /// Bottom-centered (libavm `AVM_CSP_BOTTOM`).
    Bottom,
    /// No chroma position information (libavm `AVM_CSP_UNSPECIFIED`).
    Unspecified,
}

impl ChromaSamplePosition {
    /// Convert from the raw libavm `avm_chroma_sample_position` integer.
    ///
    /// Unknown values map to [`ChromaSamplePosition::Unspecified`].
    pub fn from_raw(csp: sys::avm_chroma_sample_position) -> Self {
        match csp {
            v if v == sys::avm_chroma_sample_position_AVM_CSP_LEFT => Self::Left,
            v if v == sys::avm_chroma_sample_position_AVM_CSP_CENTER => Self::Center,
            v if v == sys::avm_chroma_sample_position_AVM_CSP_TOPLEFT => Self::TopLeft,
            v if v == sys::avm_chroma_sample_position_AVM_CSP_TOP => Self::Top,
            v if v == sys::avm_chroma_sample_position_AVM_CSP_BOTTOMLEFT => Self::BottomLeft,
            v if v == sys::avm_chroma_sample_position_AVM_CSP_BOTTOM => Self::Bottom,
            _ => Self::Unspecified,
        }
    }

    /// Returns the raw libavm `avm_chroma_sample_position` integer.
    pub fn as_raw(self) -> sys::avm_chroma_sample_position {
        match self {
            Self::Left => sys::avm_chroma_sample_position_AVM_CSP_LEFT,
            Self::Center => sys::avm_chroma_sample_position_AVM_CSP_CENTER,
            Self::TopLeft => sys::avm_chroma_sample_position_AVM_CSP_TOPLEFT,
            Self::Top => sys::avm_chroma_sample_position_AVM_CSP_TOP,
            Self::BottomLeft => sys::avm_chroma_sample_position_AVM_CSP_BOTTOMLEFT,
            Self::Bottom => sys::avm_chroma_sample_position_AVM_CSP_BOTTOM,
            Self::Unspecified => sys::avm_chroma_sample_position_AVM_CSP_UNSPECIFIED,
        }
    }
}

/// Sample value range — limited (TV / studio) or full (PC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColorRange {
    /// Studio range — Y in `[16, 235]`, chroma in `[16, 240]` for 8-bit.
    /// Maps to libavm `AVM_CR_STUDIO_RANGE`.
    Studio,
    /// Full range — Y and chroma both span `[0, 255]` for 8-bit.
    /// Maps to libavm `AVM_CR_FULL_RANGE`.
    Full,
}

impl ColorRange {
    /// Convert from the raw libavm `avm_color_range` integer.
    pub fn from_raw(cr: sys::avm_color_range) -> Self {
        if cr == sys::avm_color_range_AVM_CR_FULL_RANGE {
            Self::Full
        } else {
            Self::Studio
        }
    }

    /// Returns the raw libavm `avm_color_range` integer.
    pub fn as_raw(self) -> sys::avm_color_range {
        match self {
            Self::Studio => sys::avm_color_range_AVM_CR_STUDIO_RANGE,
            Self::Full => sys::avm_color_range_AVM_CR_FULL_RANGE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_format_round_trip() {
        for fmt in [
            PixelFormat::I420,
            PixelFormat::I422,
            PixelFormat::I444,
            PixelFormat::Yv12,
            PixelFormat::I42016,
            PixelFormat::I42216,
            PixelFormat::I44416,
            PixelFormat::Yv1216,
        ] {
            assert_eq!(PixelFormat::from_raw(fmt.as_raw()), Some(fmt));
        }
    }

    #[test]
    fn high_bit_depth_classification() {
        assert!(!PixelFormat::I420.is_high_bit_depth());
        assert!(!PixelFormat::I422.is_high_bit_depth());
        assert!(!PixelFormat::I444.is_high_bit_depth());
        assert!(!PixelFormat::Yv12.is_high_bit_depth());
        assert!(PixelFormat::I42016.is_high_bit_depth());
        assert!(PixelFormat::I42216.is_high_bit_depth());
        assert!(PixelFormat::I44416.is_high_bit_depth());
        assert!(PixelFormat::Yv1216.is_high_bit_depth());
    }

    #[test]
    fn bytes_per_sample_matches_bit_depth() {
        assert_eq!(PixelFormat::I420.bytes_per_sample(), 1);
        assert_eq!(PixelFormat::I42016.bytes_per_sample(), 2);
    }

    #[test]
    fn subsampling_classification() {
        assert_eq!(PixelFormat::I420.subsampling(), Subsampling::Yuv420);
        assert_eq!(PixelFormat::I42016.subsampling(), Subsampling::Yuv420);
        assert_eq!(PixelFormat::Yv12.subsampling(), Subsampling::Yuv420);
        assert_eq!(PixelFormat::I422.subsampling(), Subsampling::Yuv422);
        assert_eq!(PixelFormat::I42216.subsampling(), Subsampling::Yuv422);
        assert_eq!(PixelFormat::I444.subsampling(), Subsampling::Yuv444);
        assert_eq!(PixelFormat::I44416.subsampling(), Subsampling::Yuv444);
    }

    #[test]
    fn from_raw_unknown_returns_none() {
        assert_eq!(PixelFormat::from_raw(0), None);
        assert_eq!(PixelFormat::from_raw(0xDEAD_BEEF), None);
    }

    #[test]
    fn chroma_sample_position_round_trip() {
        for csp in [
            ChromaSamplePosition::Left,
            ChromaSamplePosition::Center,
            ChromaSamplePosition::TopLeft,
            ChromaSamplePosition::Top,
            ChromaSamplePosition::BottomLeft,
            ChromaSamplePosition::Bottom,
            ChromaSamplePosition::Unspecified,
        ] {
            assert_eq!(ChromaSamplePosition::from_raw(csp.as_raw()), csp);
        }
    }

    #[test]
    fn chroma_sample_position_unknown_falls_back_to_unspecified() {
        assert_eq!(
            ChromaSamplePosition::from_raw(0xFFFF_FFFF),
            ChromaSamplePosition::Unspecified,
        );
    }

    #[test]
    fn color_range_round_trip() {
        for cr in [ColorRange::Studio, ColorRange::Full] {
            assert_eq!(ColorRange::from_raw(cr.as_raw()), cr);
        }
    }
}
