use super::{library, model::*, paths::LibraryPaths, prelude::*, storage};
use crate::{
    crypto::{
        aead::{Nonce, SymmetricKey},
        hash::{ChunkHash, FileId},
    },
    file::{descriptor::FileEntry, manifest::ChunkMeta},
};

fn temp_dir_name(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("etle-{name}-{}", std::process::id()))
}

fn sample_descriptor() -> EtleDescriptor {
    EtleDescriptor::new(
        "sample-package",
        4,
        4,
        vec![FileEntry {
            path: "sample.txt".to_string(),
            size: 4,
            offset: 0,
            blake3_hash: FileId([1_u8; 32]),
        }],
        vec![ChunkMeta {
            index: 0,
            plain_size: 4,
            encrypted_size: 6,
            nonce: Nonce([2_u8; 24]),
            blake3_hash: ChunkHash([3_u8; 32]),
        }],
    )
}

#[test]
fn initializes_share_library_layout() {
    let root = temp_dir_name("state-init");
    let _ = fs::remove_dir_all(&root);
    let descriptor = sample_descriptor();
    let key = SymmetricKey([9_u8; 32]);

    let paths = library::init(
        &root,
        &descriptor,
        key,
        ShareMode::Downloading,
        Some(root.join("out")),
    )
    .unwrap();

    assert!(paths.descriptor_path().is_file());
    assert!(paths.secret_path().is_file());
    assert!(paths.progress_path().is_file());
    assert!(paths.state_path().is_file());
    assert!(paths.chunks_dir().is_dir());
    assert!(paths.output_dir().is_dir());

    assert_eq!(storage::read_descriptor(&paths).unwrap(), descriptor);
    assert_eq!(storage::read_secret(&paths).unwrap().file_key, key);
    assert_eq!(
        storage::read_progress(&paths).unwrap().completed_chunks,
        Vec::<u32>::new()
    );
    assert_eq!(
        storage::read_state(&paths).unwrap().mode,
        ShareMode::Downloading
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn lists_local_library_shares() {
    let root = temp_dir_name("state-list");
    let _ = fs::remove_dir_all(&root);
    let descriptor = sample_descriptor();
    let key = SymmetricKey([9_u8; 32]);

    let paths = library::init(&root, &descriptor, key, ShareMode::Seeding, None).unwrap();

    let shares = library::list(&root).unwrap();

    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].descriptor, descriptor);
    assert_eq!(shares[0].paths, paths);
    assert_eq!(shares[0].mode(), Some(ShareMode::Seeding));
    assert_eq!(shares[0].completed_chunks(), 1);
    assert_eq!(shares[0].total_chunks(), 1);
    assert!(shares[0].has_secret);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn progress_sorts_and_deduplicates_completed_chunks() {
    let descriptor = sample_descriptor();
    let mut progress = DownloadProgress::new(descriptor.share_id, vec![2, 1, 2, 0]);

    assert_eq!(progress.completed_chunks, vec![0, 1, 2]);
    assert!(progress.has_chunk(1));
    assert!(!progress.has_chunk(3));

    progress.mark_completed(3);
    assert_eq!(progress.completed_chunks, vec![0, 1, 2, 3]);
}

#[test]
fn encrypted_chunk_storage_roundtrip() {
    let root = temp_dir_name("state-chunk");
    let _ = fs::remove_dir_all(&root);
    let descriptor = sample_descriptor();
    let paths = LibraryPaths::for_share(&root, descriptor.share_id);
    let chunk = EncryptedChunk {
        index: 0,
        data: b"abcdef".to_vec(),
    };

    storage::write_chunk(&paths, &chunk).unwrap();

    assert!(storage::has_chunk(&paths, 0));
    assert_eq!(storage::read_chunk(&paths, 0, 6).unwrap(), chunk);
    assert!(matches!(
        storage::read_chunk(&paths, 0, 5),
        Err(FileError::ChunkSizeMismatch { index: 0, .. })
    ));

    fs::remove_dir_all(root).unwrap();
}
