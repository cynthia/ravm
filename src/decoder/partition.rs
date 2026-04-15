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

}

pub(crate) fn partition_variants(bsize: BlockSize) -> Vec<PartitionType> {
    use PartitionType::*;

    if bsize.is_min() {
        return vec![None];
    }

    let candidates: &[PartitionType] = if bsize.width == bsize.height {
        &[None, Horz, Vert, Horz3, Vert3, Horz4A, Horz4B, Vert4A, Vert4B, Split]
    } else {
        &[None, Horz, Vert, Horz3, Vert3, Split]
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
        Horz3 => {
            let top = BlockSize {
                width: bsize.width,
                height: bsize.height / 4,
            };
            let middle = BlockSize {
                width: bsize.width,
                height: bsize.height / 2,
            };
            vec![
                (x, y, top),
                (x, y + top.height, middle),
                (x, y + top.height + middle.height, top),
            ]
        }
        Vert3 => {
            let left = BlockSize {
                width: bsize.width / 4,
                height: bsize.height,
            };
            let middle = BlockSize {
                width: bsize.width / 2,
                height: bsize.height,
            };
            vec![
                (x, y, left),
                (x + left.width, y, middle),
                (x + left.width + middle.width, y, left),
            ]
        }
        Horz4A => {
            let unit = bsize.height / 8;
            let size0 = BlockSize {
                width: bsize.width,
                height: unit,
            };
            let size1 = BlockSize {
                width: bsize.width,
                height: unit * 2,
            };
            let size2 = BlockSize {
                width: bsize.width,
                height: unit * 4,
            };
            vec![
                (x, y, size0),
                (x, y + size0.height, size1),
                (x, y + size0.height + size1.height, size2),
                (x, y + size0.height + size1.height + size2.height, size0),
            ]
        }
        Horz4B => {
            let unit = bsize.height / 8;
            let size0 = BlockSize {
                width: bsize.width,
                height: unit,
            };
            let size1 = BlockSize {
                width: bsize.width,
                height: unit * 4,
            };
            let size2 = BlockSize {
                width: bsize.width,
                height: unit * 2,
            };
            vec![
                (x, y, size0),
                (x, y + size0.height, size1),
                (x, y + size0.height + size1.height, size2),
                (x, y + size0.height + size1.height + size2.height, size0),
            ]
        }
        Vert4A => {
            let unit = bsize.width / 8;
            let size0 = BlockSize {
                width: unit,
                height: bsize.height,
            };
            let size1 = BlockSize {
                width: unit * 2,
                height: bsize.height,
            };
            let size2 = BlockSize {
                width: unit * 4,
                height: bsize.height,
            };
            vec![
                (x, y, size0),
                (x + size0.width, y, size1),
                (x + size0.width + size1.width, y, size2),
                (x + size0.width + size1.width + size2.width, y, size0),
            ]
        }
        Vert4B => {
            let unit = bsize.width / 8;
            let size0 = BlockSize {
                width: unit,
                height: bsize.height,
            };
            let size1 = BlockSize {
                width: unit * 4,
                height: bsize.height,
            };
            let size2 = BlockSize {
                width: unit * 2,
                height: bsize.height,
            };
            vec![
                (x, y, size0),
                (x + size0.width, y, size1),
                (x + size0.width + size1.width, y, size2),
                (x + size0.width + size1.width + size2.width, y, size0),
            ]
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
                PartitionType::Horz3,
                vec![
                    (0, 0, BlockSize { width: 16, height: 4 }),
                    (0, 4, BlockSize { width: 16, height: 8 }),
                    (0, 12, BlockSize { width: 16, height: 4 }),
                ],
            ),
            (
                PartitionType::Vert3,
                vec![
                    (0, 0, BlockSize { width: 4, height: 16 }),
                    (4, 0, BlockSize { width: 8, height: 16 }),
                    (12, 0, BlockSize { width: 4, height: 16 }),
                ],
            ),
            (
                PartitionType::Horz4A,
                vec![
                    (0, 0, BlockSize { width: 16, height: 2 }),
                    (0, 2, BlockSize { width: 16, height: 4 }),
                    (0, 6, BlockSize { width: 16, height: 8 }),
                    (0, 14, BlockSize { width: 16, height: 2 }),
                ],
            ),
            (
                PartitionType::Horz4B,
                vec![
                    (0, 0, BlockSize { width: 16, height: 2 }),
                    (0, 2, BlockSize { width: 16, height: 8 }),
                    (0, 10, BlockSize { width: 16, height: 4 }),
                    (0, 14, BlockSize { width: 16, height: 2 }),
                ],
            ),
            (
                PartitionType::Vert4A,
                vec![
                    (0, 0, BlockSize { width: 2, height: 16 }),
                    (2, 0, BlockSize { width: 4, height: 16 }),
                    (6, 0, BlockSize { width: 8, height: 16 }),
                    (14, 0, BlockSize { width: 2, height: 16 }),
                ],
            ),
            (
                PartitionType::Vert4B,
                vec![
                    (0, 0, BlockSize { width: 2, height: 16 }),
                    (2, 0, BlockSize { width: 8, height: 16 }),
                    (10, 0, BlockSize { width: 4, height: 16 }),
                    (14, 0, BlockSize { width: 2, height: 16 }),
                ],
            ),
        ];

        for (partition, expected) in cases {
            assert_eq!(partition_children(0, 0, bsize, partition), expected);
        }
    }
}
