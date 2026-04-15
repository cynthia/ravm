#![forbid(unsafe_code)]
//! CDF tables and adaptation.

use crate::decoder::partition::BlockSize;

/// Partition CDFs keyed by neighbor context count (0, 1, 2).
///
/// `PARTITION_DO_SPLIT_CDF` and `PARTITION_DO_SQUARE_SPLIT_CDF` are ported
/// from `av2/common/entropy_inits_modes.h` for the active plane-0 path. The
/// broader multi-symbol partition tables remain placeholder-driven until the
/// full rect/ext partition syntax lands.
pub(crate) const PARTITION_DO_SPLIT_CDF: [[u16; 2]; 3] = [
    [28084, 32767],
    [23755, 32767],
    [23634, 32767],
];
pub(crate) const PARTITION_DO_SQUARE_SPLIT_CDF: [[u16; 2]; 3] = [
    [18000, 32767],
    [10521, 32767],
    [11395, 32767],
];
pub(crate) const PARTITION_RECT_TYPE_CDF: [[u16; 2]; 3] = [
    [14644, 32767],
    [10173, 32767],
    [18529, 32767],
];
pub(crate) const PARTITION_DO_EXT_CDF: [[u16; 2]; 3] = [
    [16384, 32767],
    [16384, 32767],
    [16384, 32767],
];
pub(crate) const PARTITION_DO_UNEVEN_4WAY_CDF: [[[u16; 2]; 3]; 2] = [
    [[16384, 32767], [16384, 32767], [16384, 32767]],
    [[16384, 32767], [16384, 32767], [16384, 32767]],
];
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
pub(crate) const SEGMENT_PRED_CDF: [[u16; 2]; 3] = [
    [16384, 32767],
    [16384, 32767],
    [16384, 32767],
];
pub(crate) const SPATIAL_PRED_SEG_TREE_CDF: [[u16; 8]; 3] = [
    [5622, 7893, 16093, 18233, 27809, 28373, 32533, 32767],
    [14274, 18230, 22557, 24935, 29980, 30851, 32344, 32767],
    [27527, 28487, 28723, 28890, 32397, 32647, 32679, 32767],
];
pub(crate) const DELTA_Q_CDF: [u16; 8] =
    [16594, 23325, 26424, 28225, 29358, 30099, 30613, 32767];
pub(crate) const CFL_CDF: [[u16; 2]; 3] = [
    [20441, 32767],
    [11610, 32767],
    [4643, 32767],
];
pub(crate) const CFL_INDEX_CDF: [u16; 2] = [12507, 32767];
pub(crate) const CFL_SIGN_CDF: [u16; 8] =
    [2421, 4332, 11256, 12766, 21386, 28725, 32087, 32767];
pub(crate) const CFL_ALPHA_CDF: [[u16; 8]; 6] = [
    [21679, 25305, 30646, 31512, 32537, 32646, 32696, 32767],
    [8262, 16302, 24082, 29422, 31398, 32286, 32525, 32767],
    [17235, 26166, 30378, 31305, 32373, 32549, 32668, 32767],
    [17618, 25732, 27865, 30338, 31125, 31522, 32238, 32767],
    [17542, 23066, 27907, 28728, 30702, 31165, 31435, 32767],
    [17675, 24802, 30468, 30783, 31841, 32264, 32422, 32767],
];
pub(crate) const LOSSLESS_TX_SIZE_CDF: [[[u16; 2]; 2]; 4] = [
    [[16384, 32767], [16384, 32767]],
    [[16384, 32767], [16384, 32767]],
    [[16384, 32767], [16384, 32767]],
    [[16384, 32767], [16384, 32767]],
];
pub(crate) const LOSSLESS_INTER_TX_TYPE_CDF: [u16; 2] = [16384, 32767];
pub(crate) const INTRA_EXT_TX_CDF_SET1: [[u16; 7]; 4] = [
    [5026, 16816, 19974, 23404, 26845, 30499, 32767],
    [14910, 25257, 26964, 29323, 30237, 31535, 32767],
    [13759, 26108, 27688, 29793, 30265, 31576, 32767],
    [4681, 9362, 14043, 18725, 23406, 28087, 32767],
];
pub(crate) const INTRA_EXT_TX_CDF_SET2: [[u16; 2]; 4] = [
    [16384, 32767],
    [16384, 32767],
    [16384, 32767],
    [16384, 32767],
];
pub(crate) const INTRA_EXT_TX_SHORT_SIDE_CDF: [[u16; 4]; 4] = [
    [10692, 26586, 29231, 32767],
    [26700, 32160, 32748, 32767],
    [26915, 32411, 32748, 32767],
    [8192, 16384, 24576, 32767],
];

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
pub(crate) const EOB_MULTI16_CDF: [[[u16; 5]; 3]; 4] = [
    [
        [1946, 3059, 6834, 15123, 32767],
        [558, 835, 2739, 6534, 32767],
        [8000, 10366, 14466, 19569, 32767],
    ],
    [
        [1839, 2995, 6796, 15126, 32767],
        [1354, 1787, 4813, 9327, 32767],
        [8298, 11869, 17297, 24569, 32767],
    ],
    [
        [3946, 6406, 13325, 26004, 32767],
        [2616, 3402, 7857, 13406, 32767],
        [13678, 17362, 23556, 28557, 32767],
    ],
    [
        [5029, 12046, 26017, 32453, 32767],
        [3860, 5738, 8894, 18233, 32767],
        [29060, 30345, 32212, 32705, 32767],
    ],
];
pub(crate) const COEFF_BASE_EOB_MULTI_TX4X4_CDF: [[[u16; 3]; 4]; 4] = [
    [[10923, 21845, 32767], [10923, 21845, 32767], [10923, 21845, 32767], [25475, 29789, 32767]],
    [[10923, 21845, 32767], [31077, 32105, 32767], [31889, 32651, 32767], [31276, 32538, 32767]],
    [[10923, 21845, 32767], [32132, 32735, 32767], [32555, 32745, 32767], [32458, 32729, 32767]],
    [[10923, 21845, 32767], [32389, 32626, 32767], [32642, 32756, 32767], [32629, 32756, 32767]],
];
pub(crate) const DC_SIGN_LUMA_CDF: [[[u16; 2]; 3]; 4] = [
    [[15831, 32767], [13632, 32767], [19041, 32767]],
    [[16540, 32767], [14804, 32767], [18312, 32767]],
    [[17575, 32767], [14224, 32767], [19801, 32767]],
    [[20108, 32767], [14675, 32767], [20547, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX0_CDF: [[[u16; 4]; 2]; 4] = [
    [[12360, 26392, 29943, 32767], [8590, 25353, 30895, 32767]],
    [[25298, 32426, 32713, 32767], [14651, 31551, 32591, 32767]],
    [[29506, 32636, 32731, 32767], [26854, 32392, 32720, 32767]],
    [[31676, 32740, 32753, 32767], [31095, 32715, 32748, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX1_CDF: [[[u16; 4]; 2]; 4] = [
    [[7246, 19496, 26530, 32767], [3468, 16232, 25621, 32767]],
    [[18466, 30483, 32267, 32767], [306, 26643, 31787, 32767]],
    [[23742, 32061, 32601, 32767], [2298, 30363, 32039, 32767]],
    [[29252, 32626, 32730, 32767], [5257, 31683, 32538, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX2_CDF: [[[u16; 4]; 2]; 4] = [
    [[4008, 12605, 18928, 32767], [1999, 9165, 16685, 32767]],
    [[11661, 26496, 31461, 32767], [502, 22087, 30294, 32767]],
    [[17243, 29418, 32166, 32767], [972, 24926, 30978, 32767]],
    [[25425, 31846, 32669, 32767], [2000, 27419, 31790, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX3_CDF: [[[u16; 4]; 2]; 4] = [
    [[3148, 9393, 14900, 32767], [1110, 7696, 14122, 32767]],
    [[7487, 20115, 26881, 32767], [656, 14639, 24430, 32767]],
    [[10656, 24633, 30053, 32767], [882, 19759, 28348, 32767]],
    [[10075, 24894, 29897, 32767], [2073, 19727, 27880, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX4_CDF: [[[u16; 4]; 2]; 4] = [
    [[2543, 7526, 12021, 32767], [2561, 5445, 10546, 32767]],
    [[3588, 11042, 17169, 32767], [756, 7796, 14913, 32767]],
    [[4024, 11749, 17822, 32767], [311, 8254, 15425, 32767]],
    [[3368, 9398, 13892, 32767], [571, 6307, 12048, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX5_CDF: [[[u16; 4]; 2]; 4] = [
    [[3980, 11626, 17598, 32767], [918, 8073, 15400, 32767]],
    [[4844, 14798, 22123, 32767], [839, 10591, 19419, 32767]],
    [[5051, 14703, 21347, 32767], [259, 10600, 19087, 32767]],
    [[3877, 11700, 17890, 32767], [8192, 16384, 24576, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX6_CDF: [[[u16; 4]; 2]; 4] = [
    [[3997, 11545, 16952, 32767], [146, 7698, 14688, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX7_CDF: [[[u16; 4]; 2]; 4] = [
    [[31137, 32587, 32685, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX8_CDF: [[[u16; 4]; 2]; 4] = [
    [[13651, 25517, 30395, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX9_CDF: [[[u16; 4]; 2]; 4] = [
    [[7602, 19024, 25601, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX10_CDF: [[[u16; 4]; 2]; 4] = [
    [[4692, 14076, 20351, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX11_CDF: [[[u16; 4]; 2]; 4] = [
    [[26646, 32618, 32748, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX12_CDF: [[[u16; 4]; 2]; 4] = [
    [[23711, 32006, 32620, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
];
pub(crate) const COEFF_BASE_TX4X4_CTX13_CDF: [[[u16; 4]; 2]; 4] = [
    [[16041, 28992, 31635, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
    [[8192, 16384, 24576, 32767], [8192, 16384, 24576, 32767]],
];
pub(crate) const COEFF_BR_LUMA_CTX0_CDF: [[u16; 4]; 4] = [
    [22305, 28743, 30345, 32767],
    [24190, 30560, 31576, 32767],
    [29897, 32392, 32630, 32767],
    [31501, 32621, 32738, 32767],
];
pub(crate) const COEFF_BR_LUMA_CTX1_CDF: [[u16; 4]; 4] = [
    [22663, 29948, 31320, 32767],
    [24366, 30921, 31758, 32767],
    [27484, 32113, 32486, 32767],
    [29651, 32500, 32698, 32767],
];
pub(crate) const COEFF_BR_LUMA_CTX2_CDF: [[u16; 4]; 4] = [
    [19776, 28658, 30435, 32767],
    [21555, 29858, 31096, 32767],
    [23979, 31279, 32028, 32767],
    [27204, 32153, 32586, 32767],
];
pub(crate) const COEFF_BR_LUMA_CTX3_CDF: [[u16; 4]; 4] = [
    [15436, 25313, 28181, 32767],
    [16661, 26574, 28953, 32767],
    [19126, 28997, 30760, 32767],
    [22529, 30751, 32001, 32767],
];
pub(crate) const COEFF_BR_LUMA_CTX4_CDF: [[u16; 4]; 4] = [
    [11214, 20671, 24854, 32767],
    [11988, 21738, 25479, 32767],
    [14329, 25240, 28412, 32767],
    [17048, 27341, 30583, 32767],
];

/// All-zero coefficient block marker for the walking skeleton.
pub(crate) const ALL_ZERO_CDF: [u16; 2] = [16384, 32767];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum PartitionType {
    None,
    Horz,
    Vert,
    Horz3,
    Vert3,
    Horz4A,
    Horz4B,
    Vert4A,
    Vert4B,
    Split,
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
    pub partition_do_split: [CdfState<2>; 3],
    pub partition_do_square_split: [CdfState<2>; 3],
    pub partition_rect_type: [CdfState<2>; 3],
    pub partition_do_ext: [CdfState<2>; 3],
    pub partition_do_uneven_4way: [[CdfState<2>; 3]; 2],
    pub partition_ctx: [CdfState<10>; 3],
    pub skip: CdfState<2>,
    pub segment_pred: [CdfState<2>; 3],
    pub spatial_pred_seg_tree: [CdfState<8>; 3],
    pub delta_q: CdfState<8>,
    pub cfl: [CdfState<2>; 3],
    pub cfl_index: CdfState<2>,
    pub cfl_sign: CdfState<8>,
    pub cfl_alpha: [CdfState<8>; 6],
    pub lossless_tx_size: [[CdfState<2>; 2]; 4],
    pub lossless_inter_tx_type: CdfState<2>,
    pub intra_ext_tx_set1: [CdfState<7>; 4],
    pub intra_ext_tx_set2: [CdfState<2>; 4],
    pub intra_ext_tx_short_side: [CdfState<4>; 4],
    pub y_mode_set: CdfState<4>,
    pub y_mode_idx: [CdfState<8>; 3],
    pub y_mode_idx_offset: [CdfState<6>; 3],
    pub uv_mode: [CdfState<8>; 2],
    pub eob_multi16: [[CdfState<5>; 3]; 4],
    pub coeff_base_eob_tx4x4: [[CdfState<3>; 4]; 4],
    pub dc_sign_luma: [[CdfState<2>; 3]; 4],
    pub coeff_base_tx4x4_ctx0: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx1: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx2: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx3: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx4: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx5: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx6: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx7: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx8: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx9: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx10: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx11: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx12: [[CdfState<4>; 2]; 4],
    pub coeff_base_tx4x4_ctx13: [[CdfState<4>; 2]; 4],
    pub coeff_br_luma_ctx0: [CdfState<4>; 4],
    pub coeff_br_luma_ctx1: [CdfState<4>; 4],
    pub coeff_br_luma_ctx2: [CdfState<4>; 4],
    pub coeff_br_luma_ctx3: [CdfState<4>; 4],
    pub coeff_br_luma_ctx4: [CdfState<4>; 4],
    pub all_zero: CdfState<2>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DefaultTileCdfs {
    partition_do_split: [[u16; 2]; 3],
    partition_do_square_split: [[u16; 2]; 3],
    partition_rect_type: [[u16; 2]; 3],
    partition_do_ext: [[u16; 2]; 3],
    partition_do_uneven_4way: [[[u16; 2]; 3]; 2],
    partition_ctx: [[u16; 10]; 3],
    skip: [u16; 2],
    segment_pred: [[u16; 2]; 3],
    spatial_pred_seg_tree: [[u16; 8]; 3],
    delta_q: [u16; 8],
    cfl: [[u16; 2]; 3],
    cfl_index: [u16; 2],
    cfl_sign: [u16; 8],
    cfl_alpha: [[u16; 8]; 6],
    lossless_tx_size: [[[u16; 2]; 2]; 4],
    lossless_inter_tx_type: [u16; 2],
    intra_ext_tx_set1: [[u16; 7]; 4],
    intra_ext_tx_set2: [[u16; 2]; 4],
    intra_ext_tx_short_side: [[u16; 4]; 4],
    y_mode_set: [u16; 4],
    y_mode_idx: [[u16; 8]; 3],
    y_mode_idx_offset: [[u16; 6]; 3],
    uv_mode: [[u16; 8]; 2],
    eob_multi16: [[[u16; 5]; 3]; 4],
    coeff_base_eob_tx4x4: [[[u16; 3]; 4]; 4],
    dc_sign_luma: [[[u16; 2]; 3]; 4],
    coeff_base_tx4x4_ctx0: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx1: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx2: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx3: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx4: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx5: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx6: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx7: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx8: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx9: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx10: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx11: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx12: [[[u16; 4]; 2]; 4],
    coeff_base_tx4x4_ctx13: [[[u16; 4]; 2]; 4],
    coeff_br_luma_ctx0: [[u16; 4]; 4],
    coeff_br_luma_ctx1: [[u16; 4]; 4],
    coeff_br_luma_ctx2: [[u16; 4]; 4],
    coeff_br_luma_ctx3: [[u16; 4]; 4],
    coeff_br_luma_ctx4: [[u16; 4]; 4],
    all_zero: [u16; 2],
}

impl DefaultTileCdfs {
    pub const fn new() -> Self {
        Self {
            partition_do_split: PARTITION_DO_SPLIT_CDF,
            partition_do_square_split: PARTITION_DO_SQUARE_SPLIT_CDF,
            partition_rect_type: PARTITION_RECT_TYPE_CDF,
            partition_do_ext: PARTITION_DO_EXT_CDF,
            partition_do_uneven_4way: PARTITION_DO_UNEVEN_4WAY_CDF,
            partition_ctx: [PARTITION_CDF_CTX0, PARTITION_CDF_CTX1, PARTITION_CDF_CTX2],
            skip: SKIP_CDF,
            segment_pred: SEGMENT_PRED_CDF,
            spatial_pred_seg_tree: SPATIAL_PRED_SEG_TREE_CDF,
            delta_q: DELTA_Q_CDF,
            cfl: CFL_CDF,
            cfl_index: CFL_INDEX_CDF,
            cfl_sign: CFL_SIGN_CDF,
            cfl_alpha: CFL_ALPHA_CDF,
            lossless_tx_size: LOSSLESS_TX_SIZE_CDF,
            lossless_inter_tx_type: LOSSLESS_INTER_TX_TYPE_CDF,
            intra_ext_tx_set1: INTRA_EXT_TX_CDF_SET1,
            intra_ext_tx_set2: INTRA_EXT_TX_CDF_SET2,
            intra_ext_tx_short_side: INTRA_EXT_TX_SHORT_SIDE_CDF,
            y_mode_set: Y_MODE_SET_CDF,
            y_mode_idx: Y_MODE_IDX_CDF,
            y_mode_idx_offset: Y_MODE_IDX_OFFSET_CDF,
            uv_mode: UV_MODE_CDF,
            eob_multi16: EOB_MULTI16_CDF,
            coeff_base_eob_tx4x4: COEFF_BASE_EOB_MULTI_TX4X4_CDF,
            dc_sign_luma: DC_SIGN_LUMA_CDF,
            coeff_base_tx4x4_ctx0: COEFF_BASE_TX4X4_CTX0_CDF,
            coeff_base_tx4x4_ctx1: COEFF_BASE_TX4X4_CTX1_CDF,
            coeff_base_tx4x4_ctx2: COEFF_BASE_TX4X4_CTX2_CDF,
            coeff_base_tx4x4_ctx3: COEFF_BASE_TX4X4_CTX3_CDF,
            coeff_base_tx4x4_ctx4: COEFF_BASE_TX4X4_CTX4_CDF,
            coeff_base_tx4x4_ctx5: COEFF_BASE_TX4X4_CTX5_CDF,
            coeff_base_tx4x4_ctx6: COEFF_BASE_TX4X4_CTX6_CDF,
            coeff_base_tx4x4_ctx7: COEFF_BASE_TX4X4_CTX7_CDF,
            coeff_base_tx4x4_ctx8: COEFF_BASE_TX4X4_CTX8_CDF,
            coeff_base_tx4x4_ctx9: COEFF_BASE_TX4X4_CTX9_CDF,
            coeff_base_tx4x4_ctx10: COEFF_BASE_TX4X4_CTX10_CDF,
            coeff_base_tx4x4_ctx11: COEFF_BASE_TX4X4_CTX11_CDF,
            coeff_base_tx4x4_ctx12: COEFF_BASE_TX4X4_CTX12_CDF,
            coeff_base_tx4x4_ctx13: COEFF_BASE_TX4X4_CTX13_CDF,
            coeff_br_luma_ctx0: COEFF_BR_LUMA_CTX0_CDF,
            coeff_br_luma_ctx1: COEFF_BR_LUMA_CTX1_CDF,
            coeff_br_luma_ctx2: COEFF_BR_LUMA_CTX2_CDF,
            coeff_br_luma_ctx3: COEFF_BR_LUMA_CTX3_CDF,
            coeff_br_luma_ctx4: COEFF_BR_LUMA_CTX4_CDF,
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
            partition_do_split: [
                CdfState::new(defaults.partition_do_split[0]),
                CdfState::new(defaults.partition_do_split[1]),
                CdfState::new(defaults.partition_do_split[2]),
            ],
            partition_do_square_split: [
                CdfState::new(defaults.partition_do_square_split[0]),
                CdfState::new(defaults.partition_do_square_split[1]),
                CdfState::new(defaults.partition_do_square_split[2]),
            ],
            partition_rect_type: [
                CdfState::new(defaults.partition_rect_type[0]),
                CdfState::new(defaults.partition_rect_type[1]),
                CdfState::new(defaults.partition_rect_type[2]),
            ],
            partition_do_ext: [
                CdfState::new(defaults.partition_do_ext[0]),
                CdfState::new(defaults.partition_do_ext[1]),
                CdfState::new(defaults.partition_do_ext[2]),
            ],
            partition_do_uneven_4way: [
                [
                    CdfState::new(defaults.partition_do_uneven_4way[0][0]),
                    CdfState::new(defaults.partition_do_uneven_4way[0][1]),
                    CdfState::new(defaults.partition_do_uneven_4way[0][2]),
                ],
                [
                    CdfState::new(defaults.partition_do_uneven_4way[1][0]),
                    CdfState::new(defaults.partition_do_uneven_4way[1][1]),
                    CdfState::new(defaults.partition_do_uneven_4way[1][2]),
                ],
            ],
            partition_ctx: [
                CdfState::new(defaults.partition_ctx[0]),
                CdfState::new(defaults.partition_ctx[1]),
                CdfState::new(defaults.partition_ctx[2]),
            ],
            skip: CdfState::new(defaults.skip),
            segment_pred: [
                CdfState::new(defaults.segment_pred[0]),
                CdfState::new(defaults.segment_pred[1]),
                CdfState::new(defaults.segment_pred[2]),
            ],
            spatial_pred_seg_tree: [
                CdfState::new(defaults.spatial_pred_seg_tree[0]),
                CdfState::new(defaults.spatial_pred_seg_tree[1]),
                CdfState::new(defaults.spatial_pred_seg_tree[2]),
            ],
            delta_q: CdfState::new(defaults.delta_q),
            cfl: [
                CdfState::new(defaults.cfl[0]),
                CdfState::new(defaults.cfl[1]),
                CdfState::new(defaults.cfl[2]),
            ],
            cfl_index: CdfState::new(defaults.cfl_index),
            cfl_sign: CdfState::new(defaults.cfl_sign),
            cfl_alpha: [
                CdfState::new(defaults.cfl_alpha[0]),
                CdfState::new(defaults.cfl_alpha[1]),
                CdfState::new(defaults.cfl_alpha[2]),
                CdfState::new(defaults.cfl_alpha[3]),
                CdfState::new(defaults.cfl_alpha[4]),
                CdfState::new(defaults.cfl_alpha[5]),
            ],
            lossless_tx_size: [
                [
                    CdfState::new(defaults.lossless_tx_size[0][0]),
                    CdfState::new(defaults.lossless_tx_size[0][1]),
                ],
                [
                    CdfState::new(defaults.lossless_tx_size[1][0]),
                    CdfState::new(defaults.lossless_tx_size[1][1]),
                ],
                [
                    CdfState::new(defaults.lossless_tx_size[2][0]),
                    CdfState::new(defaults.lossless_tx_size[2][1]),
                ],
                [
                    CdfState::new(defaults.lossless_tx_size[3][0]),
                    CdfState::new(defaults.lossless_tx_size[3][1]),
                ],
            ],
            lossless_inter_tx_type: CdfState::new(defaults.lossless_inter_tx_type),
            intra_ext_tx_set1: [
                CdfState::new(defaults.intra_ext_tx_set1[0]),
                CdfState::new(defaults.intra_ext_tx_set1[1]),
                CdfState::new(defaults.intra_ext_tx_set1[2]),
                CdfState::new(defaults.intra_ext_tx_set1[3]),
            ],
            intra_ext_tx_set2: [
                CdfState::new(defaults.intra_ext_tx_set2[0]),
                CdfState::new(defaults.intra_ext_tx_set2[1]),
                CdfState::new(defaults.intra_ext_tx_set2[2]),
                CdfState::new(defaults.intra_ext_tx_set2[3]),
            ],
            intra_ext_tx_short_side: [
                CdfState::new(defaults.intra_ext_tx_short_side[0]),
                CdfState::new(defaults.intra_ext_tx_short_side[1]),
                CdfState::new(defaults.intra_ext_tx_short_side[2]),
                CdfState::new(defaults.intra_ext_tx_short_side[3]),
            ],
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
            eob_multi16: [
                [
                    CdfState::new(defaults.eob_multi16[0][0]),
                    CdfState::new(defaults.eob_multi16[0][1]),
                    CdfState::new(defaults.eob_multi16[0][2]),
                ],
                [
                    CdfState::new(defaults.eob_multi16[1][0]),
                    CdfState::new(defaults.eob_multi16[1][1]),
                    CdfState::new(defaults.eob_multi16[1][2]),
                ],
                [
                    CdfState::new(defaults.eob_multi16[2][0]),
                    CdfState::new(defaults.eob_multi16[2][1]),
                    CdfState::new(defaults.eob_multi16[2][2]),
                ],
                [
                    CdfState::new(defaults.eob_multi16[3][0]),
                    CdfState::new(defaults.eob_multi16[3][1]),
                    CdfState::new(defaults.eob_multi16[3][2]),
                ],
            ],
            coeff_base_eob_tx4x4: [
                [
                    CdfState::new(defaults.coeff_base_eob_tx4x4[0][0]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[0][1]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[0][2]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[0][3]),
                ],
                [
                    CdfState::new(defaults.coeff_base_eob_tx4x4[1][0]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[1][1]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[1][2]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[1][3]),
                ],
                [
                    CdfState::new(defaults.coeff_base_eob_tx4x4[2][0]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[2][1]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[2][2]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[2][3]),
                ],
                [
                    CdfState::new(defaults.coeff_base_eob_tx4x4[3][0]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[3][1]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[3][2]),
                    CdfState::new(defaults.coeff_base_eob_tx4x4[3][3]),
                ],
            ],
            dc_sign_luma: [
                [
                    CdfState::new(defaults.dc_sign_luma[0][0]),
                    CdfState::new(defaults.dc_sign_luma[0][1]),
                    CdfState::new(defaults.dc_sign_luma[0][2]),
                ],
                [
                    CdfState::new(defaults.dc_sign_luma[1][0]),
                    CdfState::new(defaults.dc_sign_luma[1][1]),
                    CdfState::new(defaults.dc_sign_luma[1][2]),
                ],
                [
                    CdfState::new(defaults.dc_sign_luma[2][0]),
                    CdfState::new(defaults.dc_sign_luma[2][1]),
                    CdfState::new(defaults.dc_sign_luma[2][2]),
                ],
                [
                    CdfState::new(defaults.dc_sign_luma[3][0]),
                    CdfState::new(defaults.dc_sign_luma[3][1]),
                    CdfState::new(defaults.dc_sign_luma[3][2]),
                ],
            ],
            coeff_base_tx4x4_ctx0: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx0[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx0[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx0[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx0[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx0[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx0[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx0[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx0[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx1: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx1[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx1[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx1[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx1[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx1[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx1[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx1[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx1[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx2: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx2[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx2[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx2[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx2[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx2[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx2[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx2[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx2[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx3: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx3[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx3[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx3[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx3[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx3[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx3[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx3[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx3[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx4: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx4[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx4[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx4[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx4[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx4[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx4[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx4[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx4[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx5: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx5[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx5[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx5[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx5[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx5[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx5[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx5[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx5[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx6: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx6[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx6[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx6[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx6[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx6[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx6[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx6[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx6[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx7: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx7[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx7[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx7[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx7[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx7[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx7[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx7[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx7[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx8: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx8[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx8[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx8[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx8[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx8[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx8[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx8[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx8[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx9: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx9[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx9[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx9[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx9[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx9[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx9[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx9[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx9[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx10: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx10[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx10[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx10[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx10[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx10[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx10[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx10[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx10[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx11: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx11[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx11[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx11[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx11[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx11[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx11[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx11[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx11[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx12: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx12[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx12[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx12[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx12[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx12[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx12[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx12[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx12[3][1]),
                ],
            ],
            coeff_base_tx4x4_ctx13: [
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx13[0][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx13[0][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx13[1][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx13[1][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx13[2][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx13[2][1]),
                ],
                [
                    CdfState::new(defaults.coeff_base_tx4x4_ctx13[3][0]),
                    CdfState::new(defaults.coeff_base_tx4x4_ctx13[3][1]),
                ],
            ],
            coeff_br_luma_ctx0: [
                CdfState::new(defaults.coeff_br_luma_ctx0[0]),
                CdfState::new(defaults.coeff_br_luma_ctx0[1]),
                CdfState::new(defaults.coeff_br_luma_ctx0[2]),
                CdfState::new(defaults.coeff_br_luma_ctx0[3]),
            ],
            coeff_br_luma_ctx1: [
                CdfState::new(defaults.coeff_br_luma_ctx1[0]),
                CdfState::new(defaults.coeff_br_luma_ctx1[1]),
                CdfState::new(defaults.coeff_br_luma_ctx1[2]),
                CdfState::new(defaults.coeff_br_luma_ctx1[3]),
            ],
            coeff_br_luma_ctx2: [
                CdfState::new(defaults.coeff_br_luma_ctx2[0]),
                CdfState::new(defaults.coeff_br_luma_ctx2[1]),
                CdfState::new(defaults.coeff_br_luma_ctx2[2]),
                CdfState::new(defaults.coeff_br_luma_ctx2[3]),
            ],
            coeff_br_luma_ctx3: [
                CdfState::new(defaults.coeff_br_luma_ctx3[0]),
                CdfState::new(defaults.coeff_br_luma_ctx3[1]),
                CdfState::new(defaults.coeff_br_luma_ctx3[2]),
                CdfState::new(defaults.coeff_br_luma_ctx3[3]),
            ],
            coeff_br_luma_ctx4: [
                CdfState::new(defaults.coeff_br_luma_ctx4[0]),
                CdfState::new(defaults.coeff_br_luma_ctx4[1]),
                CdfState::new(defaults.coeff_br_luma_ctx4[2]),
                CdfState::new(defaults.coeff_br_luma_ctx4[3]),
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

    pub fn partition_do_split_cdf(&self, ctx: usize) -> &[u16] {
        self.partition_do_split[ctx.min(2)].as_slice()
    }

    #[allow(dead_code)]
    pub fn partition_do_square_split_cdf(&self, ctx: usize) -> &[u16] {
        self.partition_do_square_split[ctx.min(2)].as_slice()
    }

    #[cfg(test)]
    pub fn updates_enabled(&self) -> bool {
        self.updates_enabled
    }

    pub fn partition_do_uneven_4way_cdf(&self, rect_type: usize, ctx: usize) -> &[u16] {
        self.partition_do_uneven_4way[rect_type.min(1)][ctx.min(2)].as_slice()
    }

    pub fn update_skip(&mut self, symbol: usize) {
        if self.updates_enabled {
            self.skip.update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_segment_pred(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.segment_pred[ctx.min(2)].update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_segment_id(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.spatial_pred_seg_tree[ctx.min(2)].update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_delta_q(&mut self, symbol: usize) {
        if self.updates_enabled {
            self.delta_q.update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_cfl(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.cfl[ctx.min(2)].update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_cfl_index(&mut self, symbol: usize) {
        if self.updates_enabled {
            self.cfl_index.update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_cfl_sign(&mut self, symbol: usize) {
        if self.updates_enabled {
            self.cfl_sign.update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_cfl_alpha(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.cfl_alpha[ctx.min(5)].update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_lossless_tx_size(
        &mut self,
        bsize_group: usize,
        is_inter: usize,
        symbol: usize,
    ) {
        if self.updates_enabled {
            self.lossless_tx_size[bsize_group.min(3)][is_inter.min(1)].update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_lossless_inter_tx_type(&mut self, symbol: usize) {
        if self.updates_enabled {
            self.lossless_inter_tx_type.update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_intra_ext_tx_set1(&mut self, tx_size_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.intra_ext_tx_set1[tx_size_ctx.min(3)].update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_intra_ext_tx_set2(&mut self, tx_size_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.intra_ext_tx_set2[tx_size_ctx.min(3)].update(symbol);
        }
    }

    #[allow(dead_code)]
    pub fn update_intra_ext_tx_short_side(&mut self, tx_size_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.intra_ext_tx_short_side[tx_size_ctx.min(3)].update(symbol);
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

    pub fn update_eob_multi16(&mut self, q_ctx: usize, plane_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.eob_multi16[q_ctx.min(3)][plane_ctx.min(2)].update(symbol);
        }
    }

    pub fn update_coeff_base_eob_tx4x4(&mut self, q_ctx: usize, sig_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_eob_tx4x4[q_ctx.min(3)][sig_ctx.min(3)].update(symbol);
        }
    }

    pub fn update_dc_sign_luma(&mut self, q_ctx: usize, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.dc_sign_luma[q_ctx.min(3)][ctx.min(2)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx0(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx0[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx1(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx1[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx2(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx2[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx3(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx3[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx4(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx4[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx5(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx5[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx6(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx6[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx7(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx7[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx8(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx8[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx9(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx9[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx10(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx10[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx11(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx11[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx12(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx12[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_base_tx4x4_ctx13(&mut self, q_ctx: usize, tcq_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_base_tx4x4_ctx13[q_ctx.min(3)][tcq_ctx.min(1)].update(symbol);
        }
    }

    pub fn update_coeff_br_luma_ctx0(&mut self, q_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_br_luma_ctx0[q_ctx.min(3)].update(symbol);
        }
    }

    pub fn update_coeff_br_luma_ctx1(&mut self, q_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_br_luma_ctx1[q_ctx.min(3)].update(symbol);
        }
    }

    pub fn update_coeff_br_luma_ctx2(&mut self, q_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_br_luma_ctx2[q_ctx.min(3)].update(symbol);
        }
    }

    pub fn update_coeff_br_luma_ctx3(&mut self, q_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_br_luma_ctx3[q_ctx.min(3)].update(symbol);
        }
    }

    pub fn update_coeff_br_luma_ctx4(&mut self, q_ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.coeff_br_luma_ctx4[q_ctx.min(3)].update(symbol);
        }
    }

    pub fn update_partition_rect_type(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.partition_rect_type[ctx.min(2)].update(symbol);
        }
    }

    pub fn update_partition_do_square_split(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.partition_do_square_split[ctx.min(2)].update(symbol);
        }
    }

    pub fn update_partition_do_ext(&mut self, ctx: usize, symbol: usize) {
        if self.updates_enabled {
            self.partition_do_ext[ctx.min(2)].update(symbol);
        }
    }

    pub fn update_partition_do_uneven_4way(
        &mut self,
        rect_type: usize,
        ctx: usize,
        symbol: usize,
    ) {
        if self.updates_enabled {
            self.partition_do_uneven_4way[rect_type.min(1)][ctx.min(2)].update(symbol);
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
                self.partition_do_split[ctx.min(2)].update(index);
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
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_SPLIT_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_SPLIT_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_SPLIT_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_SQUARE_SPLIT_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_SQUARE_SPLIT_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_SQUARE_SPLIT_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_RECT_TYPE_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_RECT_TYPE_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_RECT_TYPE_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_EXT_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_EXT_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_EXT_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_UNEVEN_4WAY_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_UNEVEN_4WAY_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_UNEVEN_4WAY_CDF[0][2]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_UNEVEN_4WAY_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_UNEVEN_4WAY_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_DO_UNEVEN_4WAY_CDF[1][2]));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_CDF_CTX0));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_CDF_CTX1));
    out.extend_from_slice(&cdf_u16_bytes(&PARTITION_CDF_CTX2));
    out.extend_from_slice(&cdf_u16_bytes(&SKIP_CDF));
    out.extend_from_slice(&cdf_u16_bytes(&SEGMENT_PRED_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&SEGMENT_PRED_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&SEGMENT_PRED_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&SPATIAL_PRED_SEG_TREE_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&SPATIAL_PRED_SEG_TREE_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&SPATIAL_PRED_SEG_TREE_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&DELTA_Q_CDF));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_INDEX_CDF));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_SIGN_CDF));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_ALPHA_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_ALPHA_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_ALPHA_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_ALPHA_CDF[3]));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_ALPHA_CDF[4]));
    out.extend_from_slice(&cdf_u16_bytes(&CFL_ALPHA_CDF[5]));
    out.extend_from_slice(&cdf_u16_bytes(&LOSSLESS_TX_SIZE_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&LOSSLESS_TX_SIZE_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&LOSSLESS_TX_SIZE_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&LOSSLESS_TX_SIZE_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&LOSSLESS_TX_SIZE_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&LOSSLESS_TX_SIZE_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&LOSSLESS_TX_SIZE_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&LOSSLESS_TX_SIZE_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&LOSSLESS_INTER_TX_TYPE_CDF));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_CDF_SET1[0]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_CDF_SET1[1]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_CDF_SET1[2]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_CDF_SET1[3]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_CDF_SET2[0]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_CDF_SET2[1]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_CDF_SET2[2]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_CDF_SET2[3]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_SHORT_SIDE_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_SHORT_SIDE_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_SHORT_SIDE_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&INTRA_EXT_TX_SHORT_SIDE_CDF[3]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_SET_CDF));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_OFFSET_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_OFFSET_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&Y_MODE_IDX_OFFSET_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&UV_MODE_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&UV_MODE_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[0][2]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[1][2]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[2][2]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&EOB_MULTI16_CDF[3][2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[0][2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[0][3]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[1][2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[1][3]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[2][2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[2][3]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[3][2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_EOB_MULTI_TX4X4_CDF[3][3]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[0][2]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[1][2]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[2][2]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&DC_SIGN_LUMA_CDF[3][2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX0_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX0_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX0_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX0_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX0_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX0_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX0_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX0_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX1_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX1_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX1_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX1_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX1_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX1_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX1_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX1_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX2_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX2_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX2_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX2_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX2_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX2_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX2_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX2_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX3_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX3_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX3_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX3_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX3_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX3_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX3_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX3_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX4_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX4_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX4_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX4_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX4_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX4_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX4_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX4_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX5_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX5_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX5_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX5_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX5_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX5_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX5_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX5_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX6_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX6_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX6_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX6_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX6_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX6_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX6_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX6_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX7_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX7_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX7_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX7_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX7_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX7_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX7_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX7_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX8_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX8_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX8_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX8_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX8_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX8_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX8_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX8_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX9_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX9_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX9_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX9_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX9_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX9_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX9_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX9_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX10_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX10_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX10_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX10_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX10_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX10_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX10_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX10_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX11_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX11_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX11_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX11_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX11_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX11_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX11_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX11_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX12_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX12_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX12_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX12_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX12_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX12_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX12_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX12_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX13_CDF[0][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX13_CDF[0][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX13_CDF[1][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX13_CDF[1][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX13_CDF[2][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX13_CDF[2][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX13_CDF[3][0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BASE_TX4X4_CTX13_CDF[3][1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX0_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX0_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX0_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX0_CDF[3]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX1_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX1_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX1_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX1_CDF[3]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX2_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX2_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX2_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX2_CDF[3]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX3_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX3_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX3_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX3_CDF[3]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX4_CDF[0]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX4_CDF[1]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX4_CDF[2]));
    out.extend_from_slice(&cdf_u16_bytes(&COEFF_BR_LUMA_CTX4_CDF[3]));
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
        assert_eq!(tile.partition_do_split[0].as_slice(), &PARTITION_DO_SPLIT_CDF[0]);
        assert_eq!(
            tile.partition_do_square_split[0].as_slice(),
            &PARTITION_DO_SQUARE_SPLIT_CDF[0]
        );
        assert_eq!(tile.partition_rect_type[0].as_slice(), &PARTITION_RECT_TYPE_CDF[0]);
        assert_eq!(tile.partition_do_ext[0].as_slice(), &PARTITION_DO_EXT_CDF[0]);
        assert_eq!(
            tile.partition_do_uneven_4way[0][0].as_slice(),
            &PARTITION_DO_UNEVEN_4WAY_CDF[0][0]
        );
        assert_eq!(tile.segment_pred[0].as_slice(), &SEGMENT_PRED_CDF[0]);
        assert_eq!(
            tile.spatial_pred_seg_tree[0].as_slice(),
            &SPATIAL_PRED_SEG_TREE_CDF[0]
        );
        assert_eq!(tile.delta_q.as_slice(), &DELTA_Q_CDF);
        assert_eq!(tile.cfl[0].as_slice(), &CFL_CDF[0]);
        assert_eq!(tile.cfl_index.as_slice(), &CFL_INDEX_CDF);
        assert_eq!(tile.cfl_sign.as_slice(), &CFL_SIGN_CDF);
        assert_eq!(tile.cfl_alpha[0].as_slice(), &CFL_ALPHA_CDF[0]);
        assert_eq!(tile.lossless_tx_size[0][0].as_slice(), &LOSSLESS_TX_SIZE_CDF[0][0]);
        assert_eq!(
            tile.lossless_inter_tx_type.as_slice(),
            &LOSSLESS_INTER_TX_TYPE_CDF
        );
        assert_eq!(tile.intra_ext_tx_set1[0].as_slice(), &INTRA_EXT_TX_CDF_SET1[0]);
        assert_eq!(tile.intra_ext_tx_set2[0].as_slice(), &INTRA_EXT_TX_CDF_SET2[0]);
        assert_eq!(
            tile.intra_ext_tx_short_side[0].as_slice(),
            &INTRA_EXT_TX_SHORT_SIDE_CDF[0]
        );
        assert_eq!(tile.y_mode_set.as_slice(), &Y_MODE_SET_CDF);
        assert_eq!(tile.y_mode_idx[0].as_slice(), &Y_MODE_IDX_CDF[0]);
        assert_eq!(tile.uv_mode[0].as_slice(), &UV_MODE_CDF[0]);
        assert_eq!(tile.eob_multi16[0][0].as_slice(), &EOB_MULTI16_CDF[0][0]);
        assert_eq!(
            tile.coeff_base_eob_tx4x4[0][0].as_slice(),
            &COEFF_BASE_EOB_MULTI_TX4X4_CDF[0][0]
        );
        assert_eq!(tile.dc_sign_luma[0][0].as_slice(), &DC_SIGN_LUMA_CDF[0][0]);
        assert_eq!(
            tile.coeff_base_tx4x4_ctx0[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX0_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx1[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX1_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx2[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX2_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx3[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX3_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx4[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX4_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx5[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX5_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx6[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX6_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx7[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX7_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx8[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX8_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx9[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX9_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx10[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX10_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx11[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX11_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx12[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX12_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx13[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX13_CDF[0][0]
        );
        assert_eq!(tile.coeff_br_luma_ctx0[0].as_slice(), &COEFF_BR_LUMA_CTX0_CDF[0]);
        assert_eq!(tile.coeff_br_luma_ctx1[0].as_slice(), &COEFF_BR_LUMA_CTX1_CDF[0]);
        assert_eq!(tile.coeff_br_luma_ctx2[0].as_slice(), &COEFF_BR_LUMA_CTX2_CDF[0]);
        assert_eq!(tile.coeff_br_luma_ctx3[0].as_slice(), &COEFF_BR_LUMA_CTX3_CDF[0]);
        assert_eq!(tile.coeff_br_luma_ctx4[0].as_slice(), &COEFF_BR_LUMA_CTX4_CDF[0]);
    }

    #[test]
    fn tile_context_can_disable_updates() {
        let mut tile = TileContext::new(false);
        let before = tile.partition_do_split[0].as_slice().to_vec();
        tile.update_partition(
            BlockSize {
                width: 64,
                height: 64,
            },
            0,
            PartitionType::Split,
        );
        assert_eq!(tile.partition_do_split[0].as_slice(), before.as_slice());
    }

    #[test]
    fn tile_context_reset_restores_default_tables() {
        let mut tile = TileContext::new_default();
        tile.partition_do_split[0].update(1);
        tile.partition_do_square_split[0].update(1);
        tile.partition_rect_type[0].update(1);
        tile.partition_do_ext[0].update(1);
        tile.partition_do_uneven_4way[0][0].update(1);
        tile.skip.update(1);
        tile.segment_pred[0].update(1);
        tile.spatial_pred_seg_tree[0].update(3);
        tile.delta_q.update(4);
        tile.cfl[0].update(1);
        tile.cfl_index.update(1);
        tile.cfl_sign.update(2);
        tile.cfl_alpha[0].update(3);
        tile.lossless_tx_size[0][0].update(1);
        tile.lossless_inter_tx_type.update(1);
        tile.intra_ext_tx_set1[0].update(2);
        tile.intra_ext_tx_set2[0].update(1);
        tile.intra_ext_tx_short_side[0].update(2);
        tile.y_mode_set.update(1);
        tile.y_mode_idx[0].update(4);
        tile.y_mode_idx_offset[0].update(2);
        tile.uv_mode[0].update(3);
        tile.eob_multi16[0][0].update(2);
        tile.coeff_base_eob_tx4x4[0][0].update(1);
        tile.dc_sign_luma[0][0].update(1);
        tile.coeff_base_tx4x4_ctx0[0][0].update(1);
        tile.coeff_base_tx4x4_ctx1[0][0].update(1);
        tile.coeff_base_tx4x4_ctx2[0][0].update(1);
        tile.coeff_base_tx4x4_ctx3[0][0].update(1);
        tile.coeff_base_tx4x4_ctx4[0][0].update(1);
        tile.coeff_base_tx4x4_ctx5[0][0].update(1);
        tile.coeff_base_tx4x4_ctx6[0][0].update(1);
        tile.coeff_base_tx4x4_ctx7[0][0].update(1);
        tile.coeff_base_tx4x4_ctx8[0][0].update(1);
        tile.coeff_base_tx4x4_ctx9[0][0].update(1);
        tile.coeff_base_tx4x4_ctx10[0][0].update(1);
        tile.coeff_base_tx4x4_ctx11[0][0].update(1);
        tile.coeff_base_tx4x4_ctx12[0][0].update(1);
        tile.coeff_base_tx4x4_ctx13[0][0].update(1);
        tile.coeff_br_luma_ctx0[0].update(1);
        tile.coeff_br_luma_ctx1[0].update(1);
        tile.coeff_br_luma_ctx2[0].update(1);
        tile.coeff_br_luma_ctx3[0].update(1);
        tile.coeff_br_luma_ctx4[0].update(1);
        tile.all_zero.update(1);
        tile.reset_to_default();
        assert_eq!(tile.partition_do_split[0].as_slice(), &PARTITION_DO_SPLIT_CDF[0]);
        assert_eq!(
            tile.partition_do_square_split[0].as_slice(),
            &PARTITION_DO_SQUARE_SPLIT_CDF[0]
        );
        assert_eq!(tile.partition_rect_type[0].as_slice(), &PARTITION_RECT_TYPE_CDF[0]);
        assert_eq!(tile.partition_do_ext[0].as_slice(), &PARTITION_DO_EXT_CDF[0]);
        assert_eq!(
            tile.partition_do_uneven_4way[0][0].as_slice(),
            &PARTITION_DO_UNEVEN_4WAY_CDF[0][0]
        );
        assert_eq!(tile.skip.as_slice(), &SKIP_CDF);
        assert_eq!(tile.segment_pred[0].as_slice(), &SEGMENT_PRED_CDF[0]);
        assert_eq!(
            tile.spatial_pred_seg_tree[0].as_slice(),
            &SPATIAL_PRED_SEG_TREE_CDF[0]
        );
        assert_eq!(tile.delta_q.as_slice(), &DELTA_Q_CDF);
        assert_eq!(tile.cfl[0].as_slice(), &CFL_CDF[0]);
        assert_eq!(tile.cfl_index.as_slice(), &CFL_INDEX_CDF);
        assert_eq!(tile.cfl_sign.as_slice(), &CFL_SIGN_CDF);
        assert_eq!(tile.cfl_alpha[0].as_slice(), &CFL_ALPHA_CDF[0]);
        assert_eq!(tile.lossless_tx_size[0][0].as_slice(), &LOSSLESS_TX_SIZE_CDF[0][0]);
        assert_eq!(
            tile.lossless_inter_tx_type.as_slice(),
            &LOSSLESS_INTER_TX_TYPE_CDF
        );
        assert_eq!(tile.intra_ext_tx_set1[0].as_slice(), &INTRA_EXT_TX_CDF_SET1[0]);
        assert_eq!(tile.intra_ext_tx_set2[0].as_slice(), &INTRA_EXT_TX_CDF_SET2[0]);
        assert_eq!(
            tile.intra_ext_tx_short_side[0].as_slice(),
            &INTRA_EXT_TX_SHORT_SIDE_CDF[0]
        );
        assert_eq!(tile.y_mode_set.as_slice(), &Y_MODE_SET_CDF);
        assert_eq!(tile.y_mode_idx[0].as_slice(), &Y_MODE_IDX_CDF[0]);
        assert_eq!(tile.y_mode_idx_offset[0].as_slice(), &Y_MODE_IDX_OFFSET_CDF[0]);
        assert_eq!(tile.uv_mode[0].as_slice(), &UV_MODE_CDF[0]);
        assert_eq!(tile.eob_multi16[0][0].as_slice(), &EOB_MULTI16_CDF[0][0]);
        assert_eq!(
            tile.coeff_base_eob_tx4x4[0][0].as_slice(),
            &COEFF_BASE_EOB_MULTI_TX4X4_CDF[0][0]
        );
        assert_eq!(tile.dc_sign_luma[0][0].as_slice(), &DC_SIGN_LUMA_CDF[0][0]);
        assert_eq!(
            tile.coeff_base_tx4x4_ctx0[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX0_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx1[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX1_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx2[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX2_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx3[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX3_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx4[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX4_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx5[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX5_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx6[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX6_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx7[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX7_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx8[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX8_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx9[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX9_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx10[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX10_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx11[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX11_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx12[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX12_CDF[0][0]
        );
        assert_eq!(
            tile.coeff_base_tx4x4_ctx13[0][0].as_slice(),
            &COEFF_BASE_TX4X4_CTX13_CDF[0][0]
        );
        assert_eq!(tile.coeff_br_luma_ctx0[0].as_slice(), &COEFF_BR_LUMA_CTX0_CDF[0]);
        assert_eq!(tile.coeff_br_luma_ctx1[0].as_slice(), &COEFF_BR_LUMA_CTX1_CDF[0]);
        assert_eq!(tile.coeff_br_luma_ctx2[0].as_slice(), &COEFF_BR_LUMA_CTX2_CDF[0]);
        assert_eq!(tile.coeff_br_luma_ctx3[0].as_slice(), &COEFF_BR_LUMA_CTX3_CDF[0]);
        assert_eq!(tile.coeff_br_luma_ctx4[0].as_slice(), &COEFF_BR_LUMA_CTX4_CDF[0]);
        assert_eq!(tile.all_zero.as_slice(), &ALL_ZERO_CDF);
    }

    #[test]
    fn active_default_cdfs_hash_stably() {
        let digest = md5::compute(active_default_cdf_bytes());
        assert_eq!(format!("{digest:x}"), "960370397f485ad914d005dda96821ef");
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
