#![forbid(unsafe_code)]
//! Boolean arithmetic coder and symbol reading.

use crate::decoder::partition::{partition_variants, BlockSize};
use crate::decoder::symbols::{
    ALL_ZERO_CDF, INTRA_MODE_CDF, PARTITION_BINARY_CDF, PARTITION_CDF_CTX0, PARTITION_CDF_CTX1,
    PARTITION_CDF_CTX2, PartitionType, SKIP_CDF,
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
    #[cfg(test)]
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

    pub fn read_partition(&mut self, bsize: BlockSize, ctx: usize) -> PartitionType {
        let variants = partition_variants(bsize);
        if variants.len() == 1 {
            return variants[0];
        }
        let runtime_variants = runtime_partition_variants(&variants);
        if runtime_variants.len() == 2 {
            let symbol = self.read_symbol(&PARTITION_BINARY_CDF);
            return runtime_variants[symbol];
        }

        let base_cdf = match ctx.min(2) {
            0 => &PARTITION_CDF_CTX0[..],
            1 => &PARTITION_CDF_CTX1[..],
            _ => &PARTITION_CDF_CTX2[..],
        };
        let cdf = partition_cdf_for_variants(base_cdf, runtime_variants.len());
        let symbol = self.read_symbol(&cdf);
        runtime_variants[symbol]
    }

    #[allow(dead_code)]
    pub fn read_skip(&mut self) -> bool {
        self.read_symbol(&SKIP_CDF) == 1
    }

    pub fn read_intra_mode(&mut self) -> u8 {
        self.read_symbol(&INTRA_MODE_CDF) as u8
    }

    pub fn read_coeffs_4x4(&mut self, out: &mut [i16; 16]) -> Result<(), EntropyError> {
        let all_zero = self.read_symbol(&ALL_ZERO_CDF) == 0;
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

fn runtime_partition_variants(legal_variants: &[PartitionType]) -> Vec<PartitionType> {
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
        assert_eq!(
            reader.read_partition(
                BlockSize {
                    width: 64,
                    height: 64,
                },
                0,
            ),
            PartitionType::None
        );
        assert!(!reader.read_skip());
        assert_eq!(reader.read_intra_mode(), 0);
    }

    #[test]
    fn read_partition_min_block_forces_none_variant() {
        let mut reader = BacReader::new(&[0x00, 0x00, 0x00, 0x00]);
        assert_eq!(reader.read_partition(BlockSize::MIN, 2), PartitionType::None);
    }

    #[test]
    fn read_coeffs_4x4_handles_all_zero_case() {
        let mut reader = BacReader::new(&[0x00, 0x00, 0x00, 0x00]);
        let mut coeffs = [1i16; 16];
        reader.read_coeffs_4x4(&mut coeffs).expect("all-zero coeffs");
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
