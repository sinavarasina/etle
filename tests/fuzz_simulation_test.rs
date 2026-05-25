use etle::crypto::aead::{Nonce, SymmetricKey, build_chunk_aad, decrypt_chunk};
use etle::crypto::hash::FileId;

#[test]
fn fuzz_decrypt_with_random_inputs() {
    // Simulasi 1000 input acak berbeda
    for i in 0u8..=255 {
        let key = SymmetricKey([i; 32]);
        let nonce = Nonce([i; 24]);
        let file_id = FileId([0u8; 32]);

        // Input kosong
        let aad = build_chunk_aad(file_id, 0, 0);
        let _ = decrypt_chunk(&key, nonce, &[], &aad);

        // Input pendek
        let _ = decrypt_chunk(&key, nonce, &[i, i, i], &aad);

        // Input panjang dengan pola berbeda
        let data = vec![i; 1024];
        let _ = decrypt_chunk(&key, nonce, &data, &aad);
    }
    // Jika sampai sini tanpa panic = lulus
}

#[test]
fn fuzz_decrypt_truncated_tag() {
    // Ciphertext yang lebih pendek dari auth tag (16 byte) harus Err, bukan panic
    let key = SymmetricKey([1u8; 32]);
    let nonce = Nonce([2u8; 24]);
    let file_id = FileId([0u8; 32]);
    let aad = build_chunk_aad(file_id, 0, 0);

    for len in 0..16 {
        let short_data = vec![0u8; len];
        let result = decrypt_chunk(&key, nonce, &short_data, &aad);
        assert!(
            result.is_err(),
            "Harus Err untuk ciphertext pendek {len} byte"
        );
    }
}
