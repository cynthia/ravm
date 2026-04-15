#![forbid(unsafe_code)]
//! Per-4x4 block metadata used for neighbor-derived decoder contexts.

use crate::decoder::partition::BlockSize;

const LUMA_MODE_COUNT: usize = 61;
const NON_DIRECTIONAL_MODES_COUNT: u8 = 5;
const DEFAULT_MODE_LIST_Y: [u8; LUMA_MODE_COUNT - NON_DIRECTIONAL_MODES_COUNT as usize] = [
    17, 45, 3, 10, 24, 31, 38, 52, 15, 19, 43, 47, 1, 5, 8, 12, 22, 26, 29, 33, 36, 40, 50, 54,
    16, 18, 44, 46, 2, 4, 9, 11, 23, 25, 30, 32, 37, 39, 51, 53, 14, 20, 42, 48, 0, 6, 7, 13,
    21, 27, 28, 34, 35, 41, 49, 55,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct BlockInfo {
    pub present: bool,
    pub intra_mode: u8,
    pub skip: bool,
    pub tx_size: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockInfoGrid {
    cols4: usize,
    rows4: usize,
    entries: Vec<BlockInfo>,
}

impl BlockInfoGrid {
    pub fn new(frame_width: usize, frame_height: usize) -> Self {
        let cols4 = frame_width.div_ceil(4);
        let rows4 = frame_height.div_ceil(4);
        Self {
            cols4,
            rows4,
            entries: vec![BlockInfo::default(); cols4 * rows4],
        }
    }

    pub fn ctx_above(&self, bx: usize, by: usize, bsize: BlockSize) -> bool {
        if by == 0 {
            return false;
        }
        let row = (by / 4) - 1;
        let start_col = bx / 4;
        let cols = bsize.width.div_ceil(4);
        (start_col..start_col.saturating_add(cols).min(self.cols4))
            .any(|col| self.entries[self.index(col, row)].present)
    }

    pub fn ctx_left(&self, bx: usize, by: usize, bsize: BlockSize) -> bool {
        if bx == 0 {
            return false;
        }
        let col = (bx / 4) - 1;
        let start_row = by / 4;
        let rows = bsize.height.div_ceil(4);
        (start_row..start_row.saturating_add(rows).min(self.rows4))
            .any(|row| self.entries[self.index(col, row)].present)
    }

    pub fn partition_ctx(&self, bx: usize, by: usize, bsize: BlockSize) -> usize {
        usize::from(self.ctx_above(bx, by, bsize)) + usize::from(self.ctx_left(bx, by, bsize))
    }

    pub fn y_mode_ctx(&self, bx: usize, by: usize) -> usize {
        let above = self
            .entry_at_4x4((bx / 4) as isize, (by / 4) as isize - 1)
            .map(is_directional_mode)
            .unwrap_or(false);
        let left = self
            .entry_at_4x4((bx / 4) as isize - 1, (by / 4) as isize)
            .map(is_directional_mode)
            .unwrap_or(false);
        usize::from(above) + usize::from(left)
    }

    pub fn y_intra_mode_list(&self, bx: usize, by: usize, bsize: BlockSize) -> [u8; LUMA_MODE_COUNT] {
        let mut out = [0u8; LUMA_MODE_COUNT];
        let mut selected = [false; LUMA_MODE_COUNT];
        let mut mode_idx = 0usize;

        for mode in 0..usize::from(NON_DIRECTIONAL_MODES_COUNT) {
            out[mode_idx] = mode as u8;
            selected[mode] = true;
            mode_idx += 1;
        }

        if !(bsize.width < 8 || bsize.height < 8) {
            let mut neighbors = [
                self.bottom_left_joint_mode(bx, by, bsize),
                self.above_right_joint_mode(bx, by, bsize),
            ];
            let is_left_directional = neighbors[0] >= NON_DIRECTIONAL_MODES_COUNT;
            let is_above_directional = neighbors[1] >= NON_DIRECTIONAL_MODES_COUNT;
            let mut directional_mode_cnt = usize::from(is_left_directional) + usize::from(is_above_directional);
            if directional_mode_cnt == 2 && neighbors[0] == neighbors[1] {
                directional_mode_cnt = 1;
            }
            if directional_mode_cnt == 1 && !is_left_directional {
                neighbors[0] = neighbors[1];
            }
            for &neighbor in neighbors.iter().take(directional_mode_cnt) {
                let idx = neighbor as usize;
                if !selected[idx] {
                    out[mode_idx] = neighbor;
                    selected[idx] = true;
                    mode_idx += 1;
                }
            }

            if bsize.width * bsize.height > 64 {
                for delta in 0..4u8 {
                    for &neighbor in neighbors.iter().take(directional_mode_cnt) {
                        let left = (((i32::from(neighbor) - i32::from(delta) + 55) % 56) as u8)
                            + NON_DIRECTIONAL_MODES_COUNT;
                        let right = (((i32::from(neighbor) + i32::from(delta)
                            - i32::from(NON_DIRECTIONAL_MODES_COUNT - 1))
                            % 56) as u8)
                            + NON_DIRECTIONAL_MODES_COUNT;
                        for derived in [left, right] {
                            let idx = derived as usize;
                            if !selected[idx] {
                                out[mode_idx] = derived;
                                selected[idx] = true;
                                mode_idx += 1;
                            }
                        }
                    }
                }
            }
        }

        for &default_mode in &DEFAULT_MODE_LIST_Y {
            let joint_mode = default_mode + NON_DIRECTIONAL_MODES_COUNT;
            let idx = joint_mode as usize;
            if !selected[idx] && mode_idx < LUMA_MODE_COUNT {
                out[mode_idx] = joint_mode;
                selected[idx] = true;
                mode_idx += 1;
            }
        }

        out
    }

    pub fn fill_region(&mut self, bx: usize, by: usize, bsize: BlockSize, info: BlockInfo) {
        let start_col = bx / 4;
        let start_row = by / 4;
        let cols = bsize.width.div_ceil(4);
        let rows = bsize.height.div_ceil(4);

        for row in start_row..start_row.saturating_add(rows).min(self.rows4) {
            for col in start_col..start_col.saturating_add(cols).min(self.cols4) {
                let idx = self.index(col, row);
                self.entries[idx] = info;
            }
        }
    }

    fn index(&self, col: usize, row: usize) -> usize {
        row * self.cols4 + col
    }

    fn entry_at_4x4(&self, col: isize, row: isize) -> Option<BlockInfo> {
        if col < 0 || row < 0 {
            return None;
        }
        let col = col as usize;
        let row = row as usize;
        if col >= self.cols4 || row >= self.rows4 {
            return None;
        }
        let info = self.entries[self.index(col, row)];
        info.present.then_some(info)
    }

    fn bottom_left_joint_mode(&self, bx: usize, by: usize, bsize: BlockSize) -> u8 {
        self.entry_at_4x4(
            (bx / 4) as isize - 1,
            ((by + bsize.height).div_ceil(4)) as isize - 1,
        )
        .map(|info| info.intra_mode)
        .unwrap_or(0)
    }

    fn above_right_joint_mode(&self, bx: usize, by: usize, bsize: BlockSize) -> u8 {
        self.entry_at_4x4(
            ((bx + bsize.width).div_ceil(4)) as isize - 1,
            (by / 4) as isize - 1,
        )
        .map(|info| info.intra_mode)
        .unwrap_or(0)
    }
}

fn is_directional_mode(info: BlockInfo) -> bool {
    info.intra_mode >= 5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_contexts_are_absent() {
        let grid = BlockInfoGrid::new(64, 64);
        let bsize = BlockSize::MIN;
        assert!(!grid.ctx_above(0, 0, bsize));
        assert!(!grid.ctx_left(0, 0, bsize));
        assert_eq!(grid.partition_ctx(0, 0, bsize), 0);
    }

    #[test]
    fn mid_frame_contexts_see_marked_neighbors() {
        let mut grid = BlockInfoGrid::new(64, 64);
        let info = BlockInfo {
            present: true,
            intra_mode: 0,
            skip: false,
            tx_size: 0,
        };
        grid.fill_region(8, 4, BlockSize::MIN, info);
        grid.fill_region(4, 8, BlockSize::MIN, info);

        let bsize = BlockSize::MIN;
        assert!(grid.ctx_above(8, 8, bsize));
        assert!(grid.ctx_left(8, 8, bsize));
        assert_eq!(grid.partition_ctx(8, 8, bsize), 2);
    }

    #[test]
    fn y_mode_ctx_counts_directional_neighbors() {
        let mut grid = BlockInfoGrid::new(64, 64);
        let directional = BlockInfo {
            present: true,
            intra_mode: 5,
            skip: false,
            tx_size: 0,
        };
        grid.fill_region(4, 0, BlockSize::MIN, directional);
        grid.fill_region(0, 4, BlockSize::MIN, directional);
        assert_eq!(grid.y_mode_ctx(4, 4), 2);
    }

    #[test]
    fn small_blocks_use_fixed_non_directional_prefix() {
        let grid = BlockInfoGrid::new(64, 64);
        let list = grid.y_intra_mode_list(0, 0, BlockSize::MIN);
        assert_eq!(list[..5], [0, 1, 2, 3, 4]);
        assert_eq!(list[5], 22);
        assert_eq!(list[6], 50);
    }

    #[test]
    fn larger_blocks_seed_list_with_directional_neighbors() {
        let mut grid = BlockInfoGrid::new(64, 64);
        grid.fill_region(
            0,
            8,
            BlockSize::MIN,
            BlockInfo {
                present: true,
                intra_mode: 22,
                skip: false,
                tx_size: 0,
            },
        );
        grid.fill_region(
            8,
            0,
            BlockSize::MIN,
            BlockInfo {
                present: true,
                intra_mode: 50,
                skip: false,
                tx_size: 0,
            },
        );
        let list = grid.y_intra_mode_list(4, 4, BlockSize { width: 8, height: 8 });
        assert_eq!(list[..7], [0, 1, 2, 3, 4, 22, 50]);
    }
}
