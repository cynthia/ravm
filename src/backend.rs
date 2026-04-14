//! Decoder backend selection.
//!
//! The current production backend is [`BackendKind::Libavm`], which wraps the
//! upstream C decoder. [`BackendKind::Rust`] exists so the public API and test
//! harness can stabilize before the pure-Rust decoder lands.

pub(crate) mod libavm;
pub(crate) mod rust;

/// Decode backend used by [`crate::decoder::DecoderBuilder`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "bin", derive(clap::ValueEnum))]
pub enum BackendKind {
    /// Reference backend backed by upstream `libavm`.
    #[default]
    Libavm,
    /// Planned pure-Rust decoder backend.
    Rust,
}

impl BackendKind {
    /// Human-readable backend name used in logs and mismatch reports.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Libavm => "libavm",
            Self::Rust => "rust",
        }
    }
}

impl core::fmt::Display for BackendKind {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_str())
    }
}
