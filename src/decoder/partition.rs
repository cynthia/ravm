#![forbid(unsafe_code)]
//! Superblock partition tree and block-info propagation.

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct BlockSize {
    pub width: usize,
    pub height: usize,
}

impl BlockSize {
    pub const MIN: BlockSize = BlockSize {
        width: 4,
        height: 4,
    };
    pub const SB_M0: BlockSize = BlockSize {
        width: 64,
        height: 64,
    };

    pub fn is_min(self) -> bool {
        self == Self::MIN
    }

    pub fn split(self) -> Self {
        Self {
            width: self.width / 2,
            height: self.height / 2,
        }
    }
}

/// Recursively walk a superblock, visiting 4x4 leaves in Z-order.
#[cfg(test)]
pub(crate) fn walk_sb_split_only<F: FnMut(usize, usize, BlockSize)>(
    sb_x: usize,
    sb_y: usize,
    bsize: BlockSize,
    on_leaf: &mut F,
) {
    if bsize.is_min() {
        on_leaf(sb_x, sb_y, bsize);
        return;
    }

    let child = bsize.split();
    let half_w = child.width;
    let half_h = child.height;
    walk_sb_split_only(sb_x, sb_y, child, on_leaf);
    walk_sb_split_only(sb_x + half_w, sb_y, child, on_leaf);
    walk_sb_split_only(sb_x, sb_y + half_h, child, on_leaf);
    walk_sb_split_only(sb_x + half_w, sb_y + half_h, child, on_leaf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_64x64_visits_256_4x4_leaves() {
        let mut count = 0;
        walk_sb_split_only(0, 0, BlockSize::SB_M0, &mut |_, _, bs| {
            assert!(bs.is_min());
            count += 1;
        });
        assert_eq!(count, 16 * 16);
    }

    #[test]
    fn walk_visits_leaves_in_z_order() {
        let mut coords = Vec::new();
        walk_sb_split_only(0, 0, BlockSize { width: 8, height: 8 }, &mut |x, y, _| {
            coords.push((x, y));
        });
        assert_eq!(coords, vec![(0, 0), (4, 0), (0, 4), (4, 4)]);
    }
}
