//! Minimal IVF → frame loop using the streaming convenience helper.
//!
//! Usage: `cargo run --example simple_decode -- input.ivf`

use rustavm::decode_ivf;
use rustavm::ivf::IvfReader;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: simple_decode <input.ivf>");
        return ExitCode::FAILURE;
    };

    let reader = match IvfReader::open(&path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("failed to open {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "{path}: {}x{} fourcc {:?}",
        reader.header.width, reader.header.height, reader.header.fourcc
    );

    let mut count = 0usize;
    let result = decode_ivf(reader, |frame| {
        count += 1;
        println!(
            "Frame {count}: {}x{} bit_depth={} format={:?}",
            frame.width(),
            frame.height(),
            frame.bit_depth(),
            frame.format()
        );
    });

    match result {
        Ok(total) => {
            println!("Total frames decoded: {total}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("decode error: {e}");
            ExitCode::FAILURE
        }
    }
}
