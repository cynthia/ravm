use rustavm::backend::BackendKind;
use rustavm::ivf::IvfReader;
use rustavm::stream::decode_ivf_with_backend;
use std::fs;
use std::path::Path;

#[test]
fn m0_fixture_decodes_bit_exact_vs_libavm() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpora/m0/dc_intra_4x4.ivf");
    let expected_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpora/m0/dc_intra_4x4.expected.yuv");

    let mut decoded = Vec::new();
    let count = decode_ivf_with_backend(
        IvfReader::open(&path).expect("open m0 fixture"),
        BackendKind::Rust,
        None,
        |frame| {
            let owned = frame.to_owned();
            decoded.extend_from_slice(&owned.planes[0]);
            decoded.extend_from_slice(&owned.planes[1]);
            decoded.extend_from_slice(&owned.planes[2]);
        },
    )
    .expect("rust decode");
    assert_eq!(count, 1);

    let expected = y4m_payload(&fs::read(expected_path).expect("read expected y4m"));
    assert_eq!(decoded, expected);
}

fn y4m_payload(bytes: &[u8]) -> Vec<u8> {
    let mut split = bytes.splitn(3, |&b| b == b'\n');
    let _file_header = split.next().expect("y4m file header");
    let _frame_header = split.next().expect("y4m frame header");
    split.next().expect("y4m payload").to_vec()
}
