#![forbid(unsafe_code)]
//! CDF tables and adaptation.

/// Partition CDFs keyed by neighbor context count (0, 1, 2).
///
/// These are still placeholder tables; the M1 spec port will replace them with
/// the full per-block-size AV2 tables. For now the wrapper exposes the full
/// partition type set and keeps the symbol plumbing/context threading in place.
pub(crate) const PARTITION_BINARY_CDF: [u16; 2] = [16384, 32767];
pub(crate) const PARTITION_CDF_CTX0: [u16; 10] = [
    4096, 8192, 12288, 16384, 19660, 22936, 25600, 28160, 30464, 32767,
];
pub(crate) const PARTITION_CDF_CTX1: [u16; 10] = [
    6144, 11264, 15360, 19456, 22528, 25088, 27136, 29184, 30976, 32767,
];
pub(crate) const PARTITION_CDF_CTX2: [u16; 10] = [
    8192, 14336, 18432, 22016, 24576, 26624, 28416, 30208, 31488, 32767,
];

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
    Horz,
    Vert,
    Split,
    HorzA,
    HorzB,
    VertA,
    VertB,
    Horz4,
    Vert4,
}
