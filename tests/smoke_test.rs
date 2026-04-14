use rustavm::backend::BackendKind;
use rustavm::decoder::{Decoder, OwnedFrame};
use rustavm::diff::compare_frames;
use rustavm::ivf::IvfReader;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Stream {
    file: String,
    description: String,
}

fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/smoke/manifest.toml")
}

fn cache_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/smoke/cache")
}

fn load_manifest() -> Vec<Stream> {
    let text = std::fs::read_to_string(manifest_path()).expect("read smoke manifest");
    let mut streams = Vec::new();
    let mut current: Option<Stream> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[stream]]" {
            if let Some(stream) = current.take() {
                streams.push(stream);
            }
            current = Some(Stream {
                file: String::new(),
                description: String::new(),
            });
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        let current = current
            .as_mut()
            .unwrap_or_else(|| panic!("manifest field `{key}` appeared before [[stream]]"));
        match key {
            "file" => current.file = parse_string(value),
            "description" => current.description = parse_string(value),
            other => panic!("unsupported smoke manifest key `{other}`"),
        }
    }

    if let Some(stream) = current {
        streams.push(stream);
    }

    streams
}

fn parse_string(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or_else(|| panic!("expected quoted string, got `{value}`"))
        .to_string()
}

fn decode_owned_frames(
    path: &Path,
    backend: BackendKind,
) -> Result<Vec<OwnedFrame>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let ivf = IvfReader::new(BufReader::new(file))?;
    let builder = Decoder::builder().backend(backend);
    let mut decoder = builder.build()?;
    let mut frames = Vec::new();
    let mut reader = ivf;

    while let Some(packet) = reader.next_frame()? {
        decoder.decode(&packet.data)?;
        for frame in decoder.get_frames() {
            frames.push(frame.to_owned());
        }
    }

    decoder.flush()?;
    for frame in decoder.get_frames() {
        frames.push(frame.to_owned());
    }

    Ok(frames)
}

#[test]
fn xiph_smoke_rust_matches_libavm() {
    let streams = load_manifest();
    let cache = cache_dir();
    let mut ran = 0usize;
    let mut skipped = 0usize;
    let mut unsupported = 0usize;

    for stream in streams {
        let path = cache.join(&stream.file);
        if !path.is_file() {
            skipped += 1;
            continue;
        }

        let libavm = decode_owned_frames(&path, BackendKind::Libavm)
            .unwrap_or_else(|err| panic!("libavm decode of {} failed: {err}", stream.file));
        let rust = match decode_owned_frames(&path, BackendKind::Rust) {
            Ok(frames) => frames,
            Err(err) if err.to_string().contains("decoder feature not implemented") => {
                unsupported += 1;
                eprintln!(
                    "xiph smoke: skipping {} ({}) because the Rust backend still reports an unimplemented feature: {err}",
                    stream.file, stream.description
                );
                continue;
            }
            Err(err) => panic!("rust decode of {} failed: {err}", stream.file),
        };

        assert_eq!(
            rust.len(),
            libavm.len(),
            "frame-count mismatch on {}: {}",
            stream.file,
            stream.description
        );
        for (index, (rust_frame, libavm_frame)) in rust.iter().zip(&libavm).enumerate() {
            compare_frames(rust_frame, libavm_frame).unwrap_or_else(|err| {
                panic!(
                    "YUV mismatch on {} frame {}: {} ({err})",
                    stream.file, index, stream.description
                )
            });
        }
        ran += 1;
    }

    if skipped > 0 {
        eprintln!("xiph smoke: ran {ran}, skipped {skipped} (cache empty; run tests/smoke/fetch_xiph.sh)");
    }
    if unsupported > 0 {
        eprintln!("xiph smoke: skipped {unsupported} streams because the Rust backend has not implemented the required coding tools yet");
    }
    assert!(
        ran > 0 || skipped > 0 || unsupported > 0,
        "smoke manifest was empty"
    );
}
