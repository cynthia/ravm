# M0 Corpus

This directory contains the checked-in oracle stream for the pure-Rust
walking-skeleton decoder.

Files:

- `dc_intra_4x4.ivf`
- `dc_intra_4x4.expected.yuv`
- `sh.bin`
- `fh.bin`
- `tg.bin`
- `frame_obu.bin`

What is checked in:

- `dc_intra_4x4.ivf` is a single-frame 64x64 8-bit 4:2:0 reduced-still-picture
  stream.
- `dc_intra_4x4.expected.yuv` is the byte-exact output produced by the
  `libavm` backend.
- `sh.bin` and `fh.bin` are the extracted sequence-header payload and narrow
  reduced-still frame-header payload.
- `tg.bin` is the checked-in oracle tile-group payload.
- `frame_obu.bin` is the corresponding frame OBU rebuilt from `fh.bin` and
  `tg.bin`.

Regeneration:

The supported regeneration path is the local tool target:

```bash
cmake --build build-inspect --target mk_m0_final_fixture
./build-inspect/tools/mk_m0_final_fixture
```

That command regenerates:

- `tests/corpora/m0/tg.bin` from the checked-in `oracle_tg.bin`
- `tests/corpora/m0/frame_obu.bin`
- `tests/corpora/m0/dc_intra_4x4.ivf`

The regenerated IVF is expected to decode successfully with both backends:

```bash
cargo run --bin avmdec --features bin -- --backend libavm tests/corpora/m0/dc_intra_4x4.ivf /tmp/m0_libavm.yuv
cargo run --bin avmdec --features bin -- --backend rust tests/corpora/m0/dc_intra_4x4.ivf /tmp/m0_rust.yuv
cmp -s /tmp/m0_libavm.yuv tests/corpora/m0/dc_intra_4x4.expected.yuv
```

Status:

This is still the current stable fallback oracle, not the final hand-crafted
recursive-4x4-leaf stream called for by the M0 plan. The tool also has an
explicit experimental handcrafted mode, but that path is not yet treated as a
valid corpus source.
