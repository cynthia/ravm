# rustavm

Safe Rust bindings for the AV2 (`libavm`) decoder, plus a CLI (`avmdec`) for
decoding IVF streams to Y4M or raw YUV.

## Build

Requires CMake, a C/C++ toolchain, and clang (for `bindgen`). The build
script invokes CMake on the AV2 sources in this tree and links against the
resulting `libavm.a`.

```sh
cargo build                                # library only
cargo build --bin avmdec --features bin    # CLI tool
cargo test                                 # 61 tests, 24 ignored (need test data)
```

`clap` and `md5` are gated behind the `bin` feature so library consumers
do not pull in CLI-only transitive dependencies.

## CLI usage

```sh
avmdec input.ivf output.y4m              # decode to Y4M
avmdec input.ivf output.yuv --rawvideo   # decode to raw YUV
avmdec input.ivf --md5                   # print MD5 of decoded output
```

## Library usage

```rust
use rustavm::decoder::Decoder;
use rustavm::ivf::IvfReader;

let file = std::fs::File::open("input.ivf")?;
let mut ivf = IvfReader::new(file)?;
let mut decoder = Decoder::new()?;

while let Some(packet) = ivf.next_frame()? {
    decoder.decode(&packet.data)?;
    for frame in decoder.get_frames() {
        // frame.width(), frame.height(), frame.plane(0), ...
    }
}

// Drain any frames buffered for B-frame reordering.
decoder.flush()?;
for frame in decoder.get_frames() { /* trailing frames */ }
```

`Decoder` is `!Send + !Sync`. Use one decoder per thread; libavm's internal
worker pool is configured via `Decoder::with_config(Some(threads))`.

## Tests requiring data

Set `LIBAVM_TEST_DATA_PATH` to a directory containing AV2 IVF test vectors
(and `.ivf.md5` reference files for MD5 verification) and run:

```sh
LIBAVM_TEST_DATA_PATH=/path/to/testdata cargo test -- --ignored
```

## Fuzzing

```sh
cargo +nightly fuzz run ivf_decode
```

## Audit history

See [`notes/`](notes/) for the iteration-by-iteration porting plan and
security audit trail.
