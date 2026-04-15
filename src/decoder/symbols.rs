#![forbid(unsafe_code)]
//! CDF tables and adaptation.

use crate::decoder::partition::BlockSize;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CdfState<const N: usize> {
    values: [u16; N],
}

impl<const N: usize> CdfState<N> {
    pub const fn new(values: [u16; N]) -> Self {
        Self { values }
    }

    pub fn as_slice(&self) -> &[u16] {
        &self.values
    }

    pub fn update(&mut self, symbol: usize) {
        if N < 2 || symbol >= N {
            return;
        }

        for (idx, value) in self.values.iter_mut().enumerate() {
            let target = if idx < symbol { 0 } else { 32767 };
            let current = i32::from(*value);
            let delta = target - current;
            *value = (current + ((delta + delta.signum() * 7) / 8)).clamp(0, 32767) as u16;
        }
        self.values[N - 1] = 32767;
        for idx in 1..N {
            if self.values[idx - 1] > self.values[idx] {
                self.values[idx] = self.values[idx - 1];
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TileContext {
    updates_enabled: bool,
    pub partition_binary: CdfState<2>,
    pub partition_ctx: [CdfState<10>; 3],
    pub skip: CdfState<2>,
    pub intra_mode: CdfState<13>,
    pub all_zero: CdfState<2>,
}

impl TileContext {
    pub fn new_default() -> Self {
        Self::new(true)
    }

    pub fn new(updates_enabled: bool) -> Self {
        Self {
            updates_enabled,
            partition_binary: CdfState::new(PARTITION_BINARY_CDF),
            partition_ctx: [
                CdfState::new(PARTITION_CDF_CTX0),
                CdfState::new(PARTITION_CDF_CTX1),
                CdfState::new(PARTITION_CDF_CTX2),
            ],
            skip: CdfState::new(SKIP_CDF),
            intra_mode: CdfState::new(INTRA_MODE_CDF),
            all_zero: CdfState::new(ALL_ZERO_CDF),
        }
    }

    pub fn partition_cdf(&self, ctx: usize) -> &[u16] {
        self.partition_ctx[ctx.min(2)].as_slice()
    }

    pub fn updates_enabled(&self) -> bool {
        self.updates_enabled
    }

    pub fn update_partition(&mut self, bsize: BlockSize, ctx: usize, symbol: PartitionType) {
        if !self.updates_enabled {
            return;
        }
        let legal = crate::decoder::partition::partition_variants(bsize);
        let runtime = runtime_partition_variants(&legal);
        if let Some(index) = runtime.iter().position(|&entry| entry == symbol) {
            if runtime.len() == 2 {
                self.partition_binary.update(index);
            } else {
                self.partition_ctx[ctx.min(2)].update(index);
            }
        }
    }
}

pub(crate) fn runtime_partition_variants(legal_variants: &[PartitionType]) -> Vec<PartitionType> {
    use PartitionType::*;

    if legal_variants.contains(&Split) {
        return vec![None, Split];
    }
    if legal_variants.contains(&Horz) && !legal_variants.contains(&Vert) {
        return vec![None, Horz];
    }
    if legal_variants.contains(&Vert) && !legal_variants.contains(&Horz) {
        return vec![None, Vert];
    }
    vec![None, legal_variants[1]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdf_state_update_preserves_order_and_terminal() {
        let mut state = CdfState::new(PARTITION_CDF_CTX0);
        state.update(3);
        assert_eq!(state.as_slice()[state.as_slice().len() - 1], 32767);
        assert!(state.as_slice().windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn tile_context_starts_from_default_tables() {
        let tile = TileContext::new_default();
        assert!(tile.updates_enabled());
        assert_eq!(tile.partition_binary.as_slice(), &PARTITION_BINARY_CDF);
        assert_eq!(tile.intra_mode.as_slice(), &INTRA_MODE_CDF);
    }

    #[test]
    fn tile_context_can_disable_updates() {
        let mut tile = TileContext::new(false);
        let before = tile.partition_binary.as_slice().to_vec();
        tile.update_partition(
            BlockSize {
                width: 64,
                height: 64,
            },
            0,
            PartitionType::Split,
        );
        assert_eq!(tile.partition_binary.as_slice(), before.as_slice());
    }
}
