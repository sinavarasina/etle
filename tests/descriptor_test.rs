mod common;

use std::{fs, path::PathBuf};

use common::{print_banner, print_kv, print_result, print_step};
use etle::file::{
    descriptor::EtleDescriptor,
    package::{collect_package_layout, read_package_chunks},
};

fn temp_dir_name(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

#[test]
fn descriptor_can_describe_multi_file_package_layout() {
    print_banner("descriptor_can_describe_multi_file_package_layout");

    let root = temp_dir_name("descriptor-package");

    print_step(1, "create package directory");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join("intro.txt"), b"hello").unwrap();
    fs::write(root.join("assets/outro.txt"), b"world!").unwrap();
    print_kv("root", root.display());
    print_kv("files", 2);

    print_step(2, "collect package layout and chunks");
    let layout = collect_package_layout(&root).unwrap();
    let chunks = read_package_chunks(&layout, 4).unwrap();
    print_kv("layout_name", &layout.name);
    print_kv("total_size", layout.total_size);
    print_kv("chunk_count", chunks.len());

    print_step(3, "build descriptor and verify share id");
    let descriptor = EtleDescriptor::new(
        layout.name.clone(),
        layout.total_size,
        4,
        layout.descriptor_files(),
        Vec::new(),
    );
    print_kv("share_id", descriptor.share_id);
    print_kv("file_entries", descriptor.files.len());
    print_kv("share_id_valid", descriptor.verify_share_id());

    assert_eq!(layout.total_size, 11);
    assert_eq!(chunks.len(), 3);
    assert_eq!(descriptor.files.len(), 2);
    assert!(descriptor.verify_share_id());

    print_step(4, "serialize and deserialize descriptor");
    let encoded = descriptor.to_bytes().unwrap();
    let decoded = EtleDescriptor::from_bytes(&encoded).unwrap();
    print_kv("encoded_len", encoded.len());
    print_kv("decoded_equal", decoded == descriptor);
    print_kv("decoded_share_id_valid", decoded.verify_share_id());

    assert_eq!(decoded, descriptor);
    assert!(decoded.verify_share_id());

    fs::remove_dir_all(root).unwrap();
    print_result("descriptor_can_describe_multi_file_package_layout", "ok");
}
