use std::{fs, path::PathBuf};

use etle::file::{
    descriptor::EtleDescriptor,
    package::{collect_package_layout, read_package_chunks},
};

fn temp_dir_name(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

#[test]
fn descriptor_can_describe_multi_file_package_layout() {
    let root = temp_dir_name("descriptor-package");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("assets")).unwrap();
    fs::write(root.join("intro.txt"), b"hello").unwrap();
    fs::write(root.join("assets/outro.txt"), b"world!").unwrap();

    let layout = collect_package_layout(&root).unwrap();
    let chunks = read_package_chunks(&layout, 4).unwrap();
    let descriptor = EtleDescriptor::new(
        layout.name.clone(),
        layout.total_size,
        4,
        layout.descriptor_files(),
        Vec::new(),
    );

    assert_eq!(layout.total_size, 11);
    assert_eq!(chunks.len(), 3);
    assert_eq!(descriptor.files.len(), 2);
    assert!(descriptor.verify_share_id());

    let encoded = descriptor.to_bytes().unwrap();
    let decoded = EtleDescriptor::from_bytes(&encoded).unwrap();

    assert_eq!(decoded, descriptor);
    assert!(decoded.verify_share_id());

    fs::remove_dir_all(root).unwrap();
}
