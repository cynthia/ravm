use rustavm::backend::BackendKind;
use rustavm::decoder::{Decoder, Frame};
use rustavm::ivf::IvfReader;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Vector {
    file: String,
    expected_md5: String,
    features: Vec<String>,
}

fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/manifest.toml")
}

fn cache_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/conformance/kf_only")
}

fn load_manifest() -> Vec<Vector> {
    let text = std::fs::read_to_string(manifest_path()).expect("read conformance manifest");
    let mut vectors = Vec::new();
    let mut current: Option<Vector> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[vector]]" {
            if let Some(vector) = current.take() {
                vectors.push(vector);
            }
            current = Some(Vector {
                file: String::new(),
                expected_md5: String::new(),
                features: Vec::new(),
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
            .unwrap_or_else(|| panic!("manifest field `{key}` appeared before [[vector]]"));
        match key {
            "file" => current.file = parse_string(value),
            "expected_md5" => current.expected_md5 = parse_string(value),
            "features" => current.features = parse_string_array(value),
            other => panic!("unsupported conformance manifest key `{other}`"),
        }
    }

    if let Some(vector) = current {
        vectors.push(vector);
    }

    vectors
}

fn parse_string(value: &str) -> String {
    value
        .strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .unwrap_or_else(|| panic!("expected quoted string, got `{value}`"))
        .to_string()
}

fn parse_string_array(value: &str) -> Vec<String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or_else(|| panic!("expected string array, got `{value}`"))
        .trim();
    if inner.is_empty() {
        return Vec::new();
    }
    inner.split(',').map(|item| parse_string(item.trim())).collect()
}

fn frame_md5(frame: &Frame<'_>) -> String {
    let owned = frame.to_owned();
    let needs_downshift = owned.bit_depth == 8 && owned.bytes_per_sample == 2;
    let mut ctx = md5::Context::new();

    for plane in 0..3 {
        let Some(rows) = owned.rows(plane) else {
            continue;
        };
        let mut row_buf = Vec::new();
        for row in rows {
            if needs_downshift {
                let width = row.len() / owned.bytes_per_sample;
                if row_buf.len() != width {
                    row_buf.resize(width, 0);
                }
                for (x, dst) in row_buf.iter_mut().enumerate() {
                    *dst = row[x * 2];
                }
                ctx.consume(&row_buf);
            } else {
                ctx.consume(row);
            }
        }
    }

    format!("{:x}", ctx.compute())
}

fn md5_of_frames(path: &Path, backend: BackendKind) -> Result<String, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let ivf = IvfReader::new(BufReader::new(file))?;
    let builder = Decoder::builder().backend(backend);
    let mut decoder = builder.build()?;
    let mut all = String::new();
    let mut reader = ivf;

    while let Some(packet) = reader.next_frame()? {
        decoder.decode(&packet.data)?;
        for frame in decoder.get_frames() {
            all.push_str(&frame_md5(&frame));
        }
    }

    decoder.flush()?;
    for frame in decoder.get_frames() {
        all.push_str(&frame_md5(&frame));
    }

    let digest = md5::compute(all.as_bytes());
    Ok(format!("{:x}", digest))
}

#[test]
fn kf_only_conformance() {
    let vectors: Vec<_> = load_manifest()
        .into_iter()
        .filter(|vector| vector.features.iter().any(|feature| feature == "kf_only"))
        .collect();

    if vectors.is_empty() {
        eprintln!("conformance manifest is empty; add [[vector]] entries to enable this harness");
        return;
    }

    let cache = cache_dir();
    let mut skipped = 0usize;
    let mut ran = 0usize;

    for vector in vectors {
        let path = cache.join(&vector.file);
        if !path.is_file() {
            skipped += 1;
            continue;
        }

        let got = md5_of_frames(&path, BackendKind::Rust)
            .unwrap_or_else(|err| panic!("rust decode of {} failed: {err}", vector.file));
        assert_eq!(got, vector.expected_md5, "vector {} failed", vector.file);
        ran += 1;
    }

    if skipped > 0 {
        eprintln!(
            "skipped {skipped} KF-only vectors (cache empty; run tests/conformance/fetch.sh)"
        );
    }
    assert!(ran > 0 || skipped > 0, "conformance manifest was empty");
}
