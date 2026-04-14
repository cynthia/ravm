#![forbid(unsafe_code)]
//! CDF tables and adaptation.

/// Partition NONE vs SPLIT for the walking skeleton.
pub(crate) const PARTITION_NONE_SPLIT_CDF: [u16; 2] = [16384, 32767];

/// Skip flag CDF for the walking skeleton.
#[allow(dead_code)]
pub(crate) const SKIP_CDF: [u16; 2] = [29360, 32767];

/// Intra mode selector CDF. M0 only accepts DC (= 0).
pub(crate) const INTRA_MODE_CDF: [u16; 13] = [
    2521, 5042, 7563, 10084, 12605, 15126, 17647, 20168, 22689, 25210, 27731, 30252, 32767,
];

/// All-zero coefficient block marker for the walking skeleton.
pub(crate) const ALL_ZERO_CDF: [u16; 2] = [16384, 32767];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum PartitionType {
    None,
    Split,
}
