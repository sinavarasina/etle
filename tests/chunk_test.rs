mod common;

use std::{fs, path::PathBuf};

use common::{print_banner, print_kv, print_result, print_step};
use etle::file::chunker::{join_chunks, read_file_chunks};

fn temp_file_name(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-test-{name}-{}", std::process::id()))
}

#[test]
fn chunk_then_join_restores_content() {
    print_banner("chunk_then_join_restores_content");

    let input = temp_file_name("chunk-roundtrip.bin");
    let bytes = b"abcdefghi";

    print_step(1, "write sample input");
    print_kv("path", input.display());
    print_kv("input_len", bytes.len());
    fs::write(&input, bytes).unwrap();

    print_step(2, "split file into fixed-size chunks");
    let chunks = read_file_chunks(&input, 4).unwrap();
    print_kv("chunk_size", 4);
    print_kv("chunk_count", chunks.len());
    for chunk in &chunks {
        print_kv(&format!("chunk_{}_len", chunk.index), chunk.data.len());
    }

    print_step(3, "join chunks by index");
    let joined = join_chunks(&chunks);
    print_kv("joined_len", joined.len());
    print_kv("matches_input", joined == bytes);

    assert_eq!(joined, bytes);
    fs::remove_file(input).unwrap();
    print_result("chunk_then_join_restores_content", "ok");
}
