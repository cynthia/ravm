#![forbid(unsafe_code)]
//! Boolean arithmetic coder and symbol reading.

use crate::decoder::partition::{partition_variants, BlockSize};
use crate::decoder::symbols::{
    runtime_partition_variants, PartitionType, TileContext,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EntropyError {
    UnimplementedInM0,
}

pub(crate) struct BacReader<'a> {
    buf: &'a [u8],
    pos: usize,
    value: u32,
    range: u32,
    bits_left: i32,
    error: bool,
}

impl<'a> BacReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        let mut reader = Self {
            buf,
            pos: 0,
            value: 0,
            range: 0x8000,
            bits_left: -15,
            error: false,
        };
        reader.refill();
        reader
    }

    #[cfg(test)]
    pub fn had_error(&self) -> bool {
        self.error
    }

    fn refill(&mut self) {
        while self.bits_left < 0 {
            let byte = if self.pos < self.buf.len() {
                let byte = self.buf[self.pos];
                self.pos += 1;
                u32::from(byte)
            } else {
                self.error = true;
                0
            };
            self.value |= byte << ((-self.bits_left) as u32 + 8);
            self.bits_left += 8;
        }
    }

    /// Read a single equally-likely bit.
    pub fn read_bool_unbiased(&mut self) -> bool {
        self.read_symbol_binary(16384)
    }

    /// Read a symbol against an N-entry cumulative distribution.
    pub fn read_symbol(&mut self, cdf: &[u16]) -> usize {
        assert!(cdf.len() >= 2);
        assert_eq!(cdf[cdf.len() - 1], 32767);

        let mut low = 0u32;
        for (i, &high) in cdf.iter().enumerate() {
            let high = u32::from(high);
            let mid = low + ((high - low) / 2);
            let bit = self.read_symbol_binary(mid.saturating_sub(low));
            if !bit {
                return i;
            }
            low = high;
        }
        cdf.len() - 1
    }

    pub fn read_partition(
        &mut self,
        tile_ctx: &mut TileContext,
        bsize: BlockSize,
        ctx: usize,
    ) -> PartitionType {
        use PartitionType::*;

        let variants = partition_variants(bsize);
        if variants.len() == 1 {
            return variants[0];
        }

        let do_split = self.read_symbol(tile_ctx.partition_do_split_cdf(ctx));
        tile_ctx.update_partition(bsize, ctx, if do_split == 0 { None } else { Split });
        if do_split == 0 {
            return None;
        }

        if variants.contains(&Split) {
            let do_square_split = self.read_symbol(tile_ctx.partition_do_square_split_cdf(ctx));
            tile_ctx.update_partition_do_square_split(ctx, do_square_split);
            if do_square_split == 1 {
                return Split;
            }
        }

        let has_horz = variants.contains(&Horz);
        let has_vert = variants.contains(&Vert);
        if has_horz && has_vert {
            let rect_symbol = self.read_symbol(tile_ctx.partition_rect_type[ctx.min(2)].as_slice());
            tile_ctx.update_partition_rect_type(ctx, rect_symbol);
            let rect_partition = if rect_symbol == 0 { Horz } else { Vert };
            let ext_symbol = self.read_symbol(tile_ctx.partition_do_ext[ctx.min(2)].as_slice());
            tile_ctx.update_partition_do_ext(ctx, ext_symbol);
            if ext_symbol == 1 {
                let uneven_symbol =
                    self.read_symbol(tile_ctx.partition_do_uneven_4way_cdf(rect_symbol, ctx));
                tile_ctx.update_partition_do_uneven_4way(rect_symbol, ctx, uneven_symbol);
                if uneven_symbol == 1 {
                    let _uneven_type = self.read_bool_unbiased();
                    return match rect_partition {
                        Horz if variants.contains(&Horz4) => Horz4,
                        Vert if variants.contains(&Vert4) => Vert4,
                        Horz if variants.contains(&HorzA) => HorzA,
                        Vert if variants.contains(&VertA) => VertA,
                        _ => rect_partition,
                    };
                }
                return match rect_partition {
                    Horz if variants.contains(&HorzA) => HorzA,
                    Vert if variants.contains(&VertA) => VertA,
                    _ => rect_partition,
                };
            }
            return rect_partition;
        }

        let runtime_variants = runtime_partition_variants(&variants);
        if runtime_variants.len() == 2 {
            return runtime_variants[1];
        }

        let base_cdf = tile_ctx.partition_cdf(ctx);
        let cdf = partition_cdf_for_variants(base_cdf, runtime_variants.len());
        let symbol = self.read_symbol(&cdf);
        runtime_variants[symbol]
    }

    #[allow(dead_code)]
    pub fn read_skip(&mut self) -> bool {
        unreachable!("use read_skip_with_cdf")
    }

    #[allow(dead_code)]
    pub fn read_skip_with_cdf(&mut self, tile_ctx: &mut TileContext) -> bool {
        let symbol = self.read_symbol(tile_ctx.skip.as_slice());
        tile_ctx.update_skip(symbol);
        symbol == 1
    }

    pub fn read_intra_mode(&mut self, tile_ctx: &mut TileContext, ctx: usize) -> u8 {
        const LUMA_INTRA_MODE_INDEX_COUNT: u8 = 8;
        const FIRST_MODE_COUNT: u8 = 13;
        const SECOND_MODE_COUNT: u8 = 16;

        let mode_set_index = self.read_symbol(tile_ctx.y_mode_set.as_slice());
        tile_ctx.update_intra_mode(mode_set_index);
        if mode_set_index == 0 {
            let mode_idx = self.read_symbol(tile_ctx.y_mode_idx[ctx.min(2)].as_slice());
            tile_ctx.update_y_mode_idx(ctx, mode_idx);
            if mode_idx == usize::from(LUMA_INTRA_MODE_INDEX_COUNT - 1) {
                let offset = self.read_symbol(tile_ctx.y_mode_idx_offset[ctx.min(2)].as_slice());
                tile_ctx.update_y_mode_idx_offset(ctx, offset);
                (mode_idx as u8) + (offset as u8)
            } else {
                mode_idx as u8
            }
        } else {
            FIRST_MODE_COUNT + ((mode_set_index as u8) - 1) * SECOND_MODE_COUNT + self.read_literal(4)
        }
    }

    #[allow(dead_code)]
    pub fn read_uv_mode_idx(&mut self, tile_ctx: &mut TileContext, directional_y_mode: bool) -> u8 {
        const CHROMA_INTRA_MODE_INDEX_COUNT: u8 = 8;

        let ctx = usize::from(directional_y_mode);
        let uv_mode_idx = self.read_symbol(tile_ctx.uv_mode[ctx].as_slice());
        tile_ctx.update_uv_mode(ctx, uv_mode_idx);
        if uv_mode_idx == usize::from(CHROMA_INTRA_MODE_INDEX_COUNT - 1) {
            uv_mode_idx as u8 + self.read_literal(3)
        } else {
            uv_mode_idx as u8
        }
    }

    pub fn read_coeffs_4x4(
        &mut self,
        tile_ctx: &mut TileContext,
        out: &mut [i16; 16],
    ) -> Result<(), EntropyError> {
        let all_zero_symbol = self.read_symbol(tile_ctx.all_zero.as_slice());
        tile_ctx.update_all_zero(all_zero_symbol);
        let all_zero = all_zero_symbol == 0;
        if all_zero {
            *out = [0; 16];
            return Ok(());
        }
        Err(EntropyError::UnimplementedInM0)
    }

    fn read_symbol_binary(&mut self, p: u32) -> bool {
        let split = 1 + (((self.range - 1) * p) >> 15);
        let bigsplit = split << 15;
        let bit = if self.value < bigsplit {
            self.range = split;
            false
        } else {
            self.range -= split;
            self.value -= bigsplit;
            true
        };

        while self.range < 0x8000 {
            self.range <<= 1;
            self.value <<= 1;
            self.bits_left -= 1;
        }
        self.refill();
        bit
    }

    fn read_literal(&mut self, bits: u8) -> u8 {
        let mut value = 0u8;
        for _ in 0..bits {
            value = (value << 1) | u8::from(self.read_bool_unbiased());
        }
        value
    }
}

fn partition_cdf_for_variants(base_cdf: &[u16], len: usize) -> Vec<u16> {
    if len == base_cdf.len() {
        return base_cdf.to_vec();
    }

    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        if i + 1 == len {
            out.push(32767);
        } else {
            let numerator = u32::from(base_cdf[i]) * len as u32;
            let denominator = base_cdf.len() as u32;
            out.push((numerator / denominator).clamp(1, 32766) as u16);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::symbols::{TileContext, PARTITION_CDF_CTX0};

    #[test]
    fn bac_reader_on_empty_buffer_reports_error_after_read() {
        let mut reader = BacReader::new(&[]);
        let _ = reader.read_bool_unbiased();
        assert!(reader.had_error());
    }

    #[test]
    fn bac_reader_zero_buffer_decodes_stable_false_sequence() {
        let mut reader = BacReader::new(&[0x00, 0x00, 0x00, 0x00]);
        assert!(!reader.read_bool_unbiased());
        assert!(!reader.read_bool_unbiased());
        assert!(!reader.read_bool_unbiased());
    }

    #[test]
    fn read_symbol_on_two_entry_cdf_matches_binary() {
        let buf: &[u8] = &[0x00, 0x00, 0x00, 0x00];
        let mut r1 = BacReader::new(buf);
        let s1 = r1.read_symbol(&[16384, 32767]);
        let mut r2 = BacReader::new(buf);
        let s2 = if r2.read_bool_unbiased() { 1 } else { 0 };
        assert_eq!(s1, s2);
    }

    #[test]
    fn symbol_wrappers_use_cdfs() {
        let mut reader = BacReader::new(&[0x00, 0x00, 0x00, 0x00]);
        let mut tile_ctx = TileContext::new_default();
        assert_eq!(
            reader.read_partition(
                &mut tile_ctx,
                BlockSize {
                    width: 64,
                    height: 64,
                },
                0,
            ),
            PartitionType::None
        );
        assert!(!reader.read_skip_with_cdf(&mut tile_ctx));
        assert_eq!(reader.read_intra_mode(&mut tile_ctx, 0), 0);
    }

    #[test]
    fn disable_cdf_update_keeps_symbol_tables_unchanged() {
        let mut reader = BacReader::new(&[0x00, 0x00, 0x00, 0x00]);
        let mut tile_ctx = TileContext::new(false);
        let skip_before = tile_ctx.skip.as_slice().to_vec();
        let y_mode_set_before = tile_ctx.y_mode_set.as_slice().to_vec();
        let y_mode_idx_before = tile_ctx.y_mode_idx[0].as_slice().to_vec();
        let y_mode_idx_offset_before = tile_ctx.y_mode_idx_offset[0].as_slice().to_vec();
        let uv_mode_before = tile_ctx.uv_mode[0].as_slice().to_vec();
        let all_zero_before = tile_ctx.all_zero.as_slice().to_vec();

        assert!(!reader.read_skip_with_cdf(&mut tile_ctx));
        assert_eq!(reader.read_intra_mode(&mut tile_ctx, 0), 0);
        let mut coeffs = [1i16; 16];
        reader
            .read_coeffs_4x4(&mut tile_ctx, &mut coeffs)
            .expect("all-zero coeffs");
        assert_eq!(reader.read_uv_mode_idx(&mut tile_ctx, false), 0);

        assert_eq!(tile_ctx.skip.as_slice(), skip_before.as_slice());
        assert_eq!(tile_ctx.y_mode_set.as_slice(), y_mode_set_before.as_slice());
        assert_eq!(tile_ctx.y_mode_idx[0].as_slice(), y_mode_idx_before.as_slice());
        assert_eq!(
            tile_ctx.y_mode_idx_offset[0].as_slice(),
            y_mode_idx_offset_before.as_slice()
        );
        assert_eq!(tile_ctx.uv_mode[0].as_slice(), uv_mode_before.as_slice());
        assert_eq!(tile_ctx.all_zero.as_slice(), all_zero_before.as_slice());
    }

    #[test]
    fn read_uv_mode_idx_uses_real_default_table() {
        let mut reader = BacReader::new(&[0x00, 0x00, 0x00, 0x00]);
        let mut tile_ctx = TileContext::new_default();
        assert_eq!(reader.read_uv_mode_idx(&mut tile_ctx, false), 0);
    }

    #[test]
    fn read_partition_min_block_forces_none_variant() {
        let mut reader = BacReader::new(&[0x00, 0x00, 0x00, 0x00]);
        let mut tile_ctx = TileContext::new_default();
        assert_eq!(
            reader.read_partition(&mut tile_ctx, BlockSize::MIN, 2),
            PartitionType::None
        );
    }

    #[test]
    fn read_partition_non_split_rect_path_consumes_rect_type() {
        let partition = (0u8..=255).find_map(|first_byte| {
            let buf = [first_byte, 0x00, 0x00, 0x00];
            let mut reader = BacReader::new(&buf);
            let mut tile_ctx = TileContext::new_default();
            let partition = reader.read_partition(
                &mut tile_ctx,
                BlockSize {
                    width: 16,
                    height: 8,
                },
                0,
            );
            matches!(partition, PartitionType::Horz | PartitionType::Vert).then_some(partition)
        });
        assert!(
            matches!(partition, Some(PartitionType::Horz | PartitionType::Vert)),
            "no one-byte probe reached the rect non-ext path"
        );
    }

    #[test]
    fn read_partition_ext_uneven_4way_path_reaches_four_way_variant() {
        let partition = (0u8..=255).find_map(|first_byte| {
            (0u8..=255).find_map(|second_byte| {
                let buf = [first_byte, second_byte, 0x00, 0x00];
                let mut reader = BacReader::new(&buf);
                let mut tile_ctx = TileContext::new_default();
                let partition = reader.read_partition(
                    &mut tile_ctx,
                    BlockSize {
                        width: 16,
                        height: 16,
                    },
                    0,
                );
                matches!(partition, PartitionType::Horz4 | PartitionType::Vert4)
                    .then_some(partition)
            })
        });
        assert!(
            matches!(partition, Some(PartitionType::Horz4 | PartitionType::Vert4)),
            "no two-byte probe reached the ext uneven-4way path"
        );
    }

    #[test]
    fn read_coeffs_4x4_handles_all_zero_case() {
        let mut reader = BacReader::new(&[0x00, 0x00, 0x00, 0x00]);
        let mut tile_ctx = TileContext::new_default();
        let mut coeffs = [1i16; 16];
        reader
            .read_coeffs_4x4(&mut tile_ctx, &mut coeffs)
            .expect("all-zero coeffs");
        assert_eq!(coeffs, [0i16; 16]);
    }

    #[test]
    fn partition_cdf_helper_preserves_terminal_value() {
        let cdf = partition_cdf_for_variants(&PARTITION_CDF_CTX0, 8);
        assert_eq!(cdf.len(), 8);
        assert_eq!(cdf[7], 32767);
    }

    #[test]
    fn runtime_partition_variants_preserve_binary_split_path_when_available() {
        let variants = vec![
            PartitionType::None,
            PartitionType::Horz,
            PartitionType::Vert,
            PartitionType::Split,
        ];
        assert_eq!(
            runtime_partition_variants(&variants),
            vec![PartitionType::None, PartitionType::Split]
        );
    }
}
