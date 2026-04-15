#![forbid(unsafe_code)]
//! Per-4x4 block metadata used for neighbor-derived decoder contexts.

use crate::decoder::partition::BlockSize;

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
}
