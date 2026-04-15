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

/// Luma intra-mode selector CDFs from `av2/common/entropy_inits_modes.h`.
pub(crate) const Y_MODE_SET_CDF: [u16; 4] = [28863, 31022, 31724, 32767];
pub(crate) const Y_MODE_IDX_CDF: [[u16; 8]; 3] = [
    [15175, 20075, 21728, 24098, 26405, 27655, 28860, 32767],
    [10114, 14957, 16815, 19127, 20147, 25583, 27169, 32767],
    [5636, 9004, 10456, 12122, 12744, 20325, 25607, 32767],
];
pub(crate) const Y_MODE_IDX_OFFSET_CDF: [[u16; 6]; 3] = [
    [12743, 18172, 20194, 23648, 26419, 32767],
    [8976, 16084, 20827, 24595, 28496, 32767],
    [8784, 14556, 19710, 24903, 28724, 32767],
];
pub(crate) const UV_MODE_CDF: [[u16; 8]; 2] = [
    [9363, 20957, 22865, 24753, 26411, 27983, 30428, 32767],
    [21282, 23610, 28208, 29311, 30348, 31158, 31491, 32767],
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
    pub y_mode_set: CdfState<4>,
    pub y_mode_idx: [CdfState<8>; 3],
    pub y_mode_idx_offset: [CdfState<6>; 3],
    pub uv_mode: [CdfState<8>; 2],
    pub all_zero: CdfState<2>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefaultTileCdfs {
    partition_binary: [u16; 2],
    partition_ctx: [[u16; 10]; 3],
    skip: [u16; 2],
    y_mode_set: [u16; 4],
    y_mode_idx: [[u16; 8]; 3],
    y_mode_idx_offset: [[u16; 6]; 3],
    uv_mode: [[u16; 8]; 2],
    all_zero: [u16; 2],
}

impl DefaultTileCdfs {
    pub const fn new() -> Self {
        Self {
            partition_binary: PARTITION_BINARY_CDF,
            partition_ctx: [PARTITION_CDF_CTX0, PARTITION_CDF_CTX1, PARTITION_CDF_CTX2],
            skip: SKIP_CDF,
            y_mode_set: Y_MODE_SET_CDF,
            y_mode_idx: Y_MODE_IDX_CDF,
            y_mode_idx_offset: Y_MODE_IDX_OFFSET_CDF,
            uv_mode: UV_MODE_CDF,
            all_zero: ALL_ZERO_CDF,
        }
    }
}

impl TileContext {
    pub fn new_default() -> Self {
        Self::new(true)
    }

    pub fn new(updates_enabled: bool) -> Self {
        Self::from_defaults(DefaultTileCdfs::new(), updates_enabled)
    }

    pub fn from_defaults(defaults: DefaultTileCdfs, updates_enabled: bool) -> Self {
        Self {
            updates_enabled,
            partition_binary: CdfState::new(defaults.partition_binary),
            partition_ctx: [
                CdfState::new(defaults.partition_ctx[0]),
                CdfState::new(defaults.partition_ctx[1]),
                CdfState::new(defaults.partition_ctx[2]),
            ],
            skip: CdfState::new(defaults.skip),
            y_mode_set: CdfState::new(defaults.y_mode_set),
            y_mode_idx: [
                CdfState::new(defaults.y_mode_idx[0]),
                CdfState::new(defaults.y_mode_idx[1]),
                CdfState::new(defaults.y_mode_idx[2]),
            ],
            y_mode_idx_offset: [
                CdfState::new(defaults.y_mode_idx_offset[0]),
                CdfState::new(defaults.y_mode_idx_offset[1]),
                CdfState::new(defaults.y_mode_idx_offset[2]),
            ],
            uv_mode: [
                CdfState::new(defaults.uv_mode[0]),
                CdfState::new(defaults.uv_mode[1]),
            ],
            all_zero: CdfState::new(defaults.all_zero),
        }
    }

    #[cfg(test)]
    pub fn reset_to_default(&mut self) {
        let updates_enabled = self.updates_enabled;
        *self = Self::from_defaults(DefaultTileCdfs::new(), updates_enabled);
    }

    pub fn partition_cdf(&self, ctx: usize) -> &[u16] {
        self.partition_ctx[ctx.min(2)].as_slice()
    }

    #[cfg(test)]
    pub fn updates_enabled(&self) -> bool {
        self.updates_enabled
    }

    pub fn update_skip(&mut self, symbol: usize) {
        if self.updates_enabled {
            self.skip.update(symbol);
        }
    }

    pub fn update_intra_mode(&mut self, symbol: usize) {
        if self.updates_enabled {
            self.y_mode_set.update(symbol);
        }
    }

    pub fn update_y_mode_idx(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.y_mode_idx[ctx.min(2)].update(symbol);
        }
    }

    pub fn update_y_mode_idx_offset(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.y_mode_idx_offset[ctx.min(2)].update(symbol);
        }
    }

    pub fn update_uv_mode(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.uv_mode[ctx.min(1)].update(symbol);
        }
    }

    pub fn update_all_zero(&mut self, symbol: usize) {
        if self.updates_enabled {
            self.all_zero.update(symbol);
        }
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

#[cfg(test)]
fn cdf_u16_bytes(cdf: &[u16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(cdf.len() * 2);
    for value in cdf {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

#[cfg(test)]
fn active_default_cdf_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_BINARY_CDF));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_CDF_CTX0));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_CDF_CTX1));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_CDF_CTX2));
    out.extend_from_slice(&cdf_u16_bytes(&SKIP_CDF));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_SET_CDF));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_OFFSET_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_OFFSET_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_OFFSET_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&UV_MODE_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&UV_MODE_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&ALL_ZERO_CDF));
    out
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
        assert_eq!(tile.y_mode_set.as_slice(), &Y_MODE_SET_CDF);
        assert_eq!(tile.y_mode_idx[0].as_slice(), &Y_MODE_IDX_CDF[0]);
        assert_eq!(tile.uv_mode[0].as_slice(), &UV_MODE_CDF[0]);
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

    #[test]
    fn tile_context_reset_restores_default_tables() {
        let mut tile = TileContext::new_default();
        tile.partition_binary.update(1);
        tile.skip.update(1);
        tile.y_mode_set.update(1);
        tile.y_mode_idx[0].update(4);
        tile.y_mode_idx_offset[0].update(2);
        tile.uv_mode[0].update(3);
        tile.all_zero.update(1);
        tile.reset_to_default();
        assert_eq!(tile.partition_binary.as_slice(), &PARTITION_BINARY_CDF);
        assert_eq!(tile.skip.as_slice(), &SKIP_CDF);
        assert_eq!(tile.y_mode_set.as_slice(), &Y_MODE_SET_CDF);
        assert_eq!(tile.y_mode_idx[0].as_slice(), &Y_MODE_IDX_CDF[0]);
        assert_eq!(tile.y_mode_idx_offset[0].as_slice(), &Y_MODE_IDX_OFFSET_CDF[0]);
        assert_eq!(tile.uv_mode[0].as_slice(), &UV_MODE_CDF[0]);
        assert_eq!(tile.all_zero.as_slice(), &ALL_ZERO_CDF);
    }

    #[test]
    fn active_default_cdfs_hash_stably() {
        let digest = md5::compute(active_default_cdf_bytes());
        assert_eq!(format!("{digest:x}"), "c896fd58dc5b57a41fa5081f184be3f5");
    }

    #[test]
    fn partition_context_tables_hash_stably_per_family() {
        let partition_bytes = [
            cdf_u16_bytes(&PARTITION_CDF_CTX0),
            cdf_u16_bytes(&PARTITION_CDF_CTX1),
            cdf_u16_bytes(&PARTITION_CDF_CTX2),
        ]
        .concat();
        let digest = md5::compute(partition_bytes);
        assert_eq!(format!("{digest:x}"), "e8478417b5bfc0e2e9252c5f7714eafe");
    }
}
