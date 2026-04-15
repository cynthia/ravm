#![forbid(unsafe_code)]
//! Superblock partition tree and block-info propagation.

use crate::decoder::symbols::PartitionType;

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

    pub fn quarter_h(self) -> Self {
        Self {
            width: self.width,
            height: self.height / 4,
        }
    }

    pub fn quarter_w(self) -> Self {
        Self {
            width: self.width / 4,
            height: self.height,
        }
    }
}

pub(crate) fn partition_variants(bsize: BlockSize) -> Vec<PartitionType> {
    use PartitionType::*;

    if bsize.is_min() {
        return vec![None];
    }

    let candidates: &[PartitionType] = if bsize.width == bsize.height {
        &[None, Horz, Vert, Split, HorzA, HorzB, VertA, VertB, Horz4, Vert4]
    } else {
        &[None, Horz, Vert, Split, HorzA, HorzB, VertA, VertB]
    };

    candidates
        .iter()
        .copied()
        .filter(|&partition| partition_is_legal(bsize, partition))
        .collect()
}

pub(crate) fn partition_children(
    x: usize,
    y: usize,
    bsize: BlockSize,
    partition: PartitionType,
) -> Vec<(usize, usize, BlockSize)> {
    use PartitionType::*;

    match partition {
        None => vec![(x, y, bsize)],
        Split => {
            let child = bsize.split();
            vec![
                (x, y, child),
                (x + child.width, y, child),
                (x, y + child.height, child),
                (x + child.width, y + child.height, child),
            ]
        }
        Horz => {
            let child = BlockSize {
                width: bsize.width,
                height: bsize.height / 2,
            };
            vec![(x, y, child), (x, y + child.height, child)]
        }
        Vert => {
            let child = BlockSize {
                width: bsize.width / 2,
                height: bsize.height,
            };
            vec![(x, y, child), (x + child.width, y, child)]
        }
        HorzA => {
            let split = bsize.split();
            let bottom = BlockSize {
                width: bsize.width,
                height: split.height,
            };
            vec![
                (x, y, split),
                (x + split.width, y, split),
                (x, y + split.height, bottom),
            ]
        }
        HorzB => {
            let split = bsize.split();
            let top = BlockSize {
                width: bsize.width,
                height: split.height,
            };
            vec![
                (x, y, top),
                (x, y + split.height, split),
                (x + split.width, y + split.height, split),
            ]
        }
        VertA => {
            let split = bsize.split();
            let right = BlockSize {
                width: split.width,
                height: bsize.height,
            };
            vec![
                (x, y, split),
                (x, y + split.height, split),
                (x + split.width, y, right),
            ]
        }
        VertB => {
            let split = bsize.split();
            let left = BlockSize {
                width: split.width,
                height: bsize.height,
            };
            vec![
                (x, y, left),
                (x + split.width, y, split),
                (x + split.width, y + split.height, split),
            ]
        }
        Horz4 => {
            let child = bsize.quarter_h();
            (0..4)
                .map(|i| (x, y + i * child.height, child))
                .collect()
        }
        Vert4 => {
            let child = bsize.quarter_w();
            (0..4)
                .map(|i| (x + i * child.width, y, child))
                .collect()
        }
    }
}

#[cfg(test)]
pub(crate) fn walk_partition_tree<D, V, E>(
    x: usize,
    y: usize,
    bsize: BlockSize,
    decide_partition: &mut D,
    visit_leaf: &mut V,
) -> Result<(), E>
where
    D: FnMut(usize, usize, BlockSize) -> Result<PartitionType, E>,
    V: FnMut(usize, usize, BlockSize, PartitionType) -> Result<(), E>,
{
    let partition = if bsize.is_min() {
        PartitionType::None
    } else {
        decide_partition(x, y, bsize)?
    };

    if partition == PartitionType::None || bsize.is_min() {
        return visit_leaf(x, y, bsize, partition);
    }

    for (child_x, child_y, child_size) in partition_children(x, y, bsize, partition) {
        walk_partition_tree(child_x, child_y, child_size, decide_partition, visit_leaf)?;
    }
    Ok(())
}

fn partition_is_legal(bsize: BlockSize, partition: PartitionType) -> bool {
    if partition == PartitionType::None {
        return true;
    }

    partition_children(0, 0, bsize, partition)
        .into_iter()
        .all(|(_, _, child)| {
            child.width >= 4
                && child.height >= 4
                && (child.width < bsize.width || child.height < bsize.height)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_64x64_visits_256_4x4_leaves() {
        let mut count = 0;
        walk_partition_tree(
            0,
            0,
            BlockSize::SB_M0,
            &mut |_, _, _| Ok::<_, ()>(PartitionType::Split),
            &mut |_, _, bs, _| {
                assert!(bs.is_min());
                count += 1;
                Ok(())
            },
        )
        .expect("walk");
        assert_eq!(count, 16 * 16);
    }

    #[test]
    fn walk_visits_leaves_in_z_order() {
        let mut coords = Vec::new();
        walk_partition_tree(
            0,
            0,
            BlockSize { width: 8, height: 8 },
            &mut |_, _, _| Ok::<_, ()>(PartitionType::Split),
            &mut |x, y, _, _| {
                coords.push((x, y));
                Ok(())
            },
        )
        .expect("walk");
        assert_eq!(coords, vec![(0, 0), (4, 0), (0, 4), (4, 4)]);
    }

    #[test]
    fn partition_children_match_expected_shapes() {
        let bsize = BlockSize {
            width: 16,
            height: 16,
        };
        let cases = [
            (
                PartitionType::Horz,
                vec![
                    (0, 0, BlockSize { width: 16, height: 8 }),
                    (0, 8, BlockSize { width: 16, height: 8 }),
                ],
            ),
            (
                PartitionType::Vert,
                vec![
                    (0, 0, BlockSize { width: 8, height: 16 }),
                    (8, 0, BlockSize { width: 8, height: 16 }),
                ],
            ),
            (
                PartitionType::HorzA,
                vec![
                    (0, 0, BlockSize { width: 8, height: 8 }),
                    (8, 0, BlockSize { width: 8, height: 8 }),
                    (0, 8, BlockSize { width: 16, height: 8 }),
                ],
            ),
            (
                PartitionType::HorzB,
                vec![
                    (0, 0, BlockSize { width: 16, height: 8 }),
                    (0, 8, BlockSize { width: 8, height: 8 }),
                    (8, 8, BlockSize { width: 8, height: 8 }),
                ],
            ),
            (
                PartitionType::VertA,
                vec![
                    (0, 0, BlockSize { width: 8, height: 8 }),
                    (0, 8, BlockSize { width: 8, height: 8 }),
                    (8, 0, BlockSize { width: 8, height: 16 }),
                ],
            ),
            (
                PartitionType::VertB,
                vec![
                    (0, 0, BlockSize { width: 8, height: 16 }),
                    (8, 0, BlockSize { width: 8, height: 8 }),
                    (8, 8, BlockSize { width: 8, height: 8 }),
                ],
            ),
            (
                PartitionType::Horz4,
                vec![
                    (0, 0, BlockSize { width: 16, height: 4 }),
                    (0, 4, BlockSize { width: 16, height: 4 }),
                    (0, 8, BlockSize { width: 16, height: 4 }),
                    (0, 12, BlockSize { width: 16, height: 4 }),
                ],
            ),
            (
                PartitionType::Vert4,
                vec![
                    (0, 0, BlockSize { width: 4, height: 16 }),
                    (4, 0, BlockSize { width: 4, height: 16 }),
                    (8, 0, BlockSize { width: 4, height: 16 }),
                    (12, 0, BlockSize { width: 4, height: 16 }),
                ],
            ),
        ];

        for (partition, expected) in cases {
            assert_eq!(partition_children(0, 0, bsize, partition), expected);
        }
    }
}
