use std::{fs, path::PathBuf};

use etle::file::chunker::{join_chunks, read_file_chunks};

fn temp_file_name(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-test-{name}-{}", std::process::id()))
}

#[test]
fn chunk_then_join_restores_content() {
    let input = temp_file_name("chunk-roundtrip.bin");
    let bytes = b"abcdefghi";
    fs::write(&input, bytes).unwrap();

    let chunks = read_file_chunks(&input, 4).unwrap();
    let joined = join_chunks(&chunks);

    assert_eq!(joined, bytes);
    fs::remove_file(input).unwrap();
}
