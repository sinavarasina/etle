use etle::crypto::{
    aead::{Nonce, SymmetricKey, build_chunk_aad, decrypt_chunk},
    hash::FileId,
};

#[test]
fn fuzz_decrypt_with_random_inputs() {
    for value in 0_u8..=255 {
        let key = SymmetricKey([value; 32]);
        let nonce = Nonce([value; 24]);
        let file_id = FileId([0_u8; 32]);
        let aad = build_chunk_aad(file_id, 0, 0);

        let _ = decrypt_chunk(&key, nonce, &[], &aad);
        let _ = decrypt_chunk(&key, nonce, &[value, value, value], &aad);

        let data = vec![value; 1024];
        let _ = decrypt_chunk(&key, nonce, &data, &aad);
    }
}

#[test]
fn fuzz_decrypt_truncated_tag() {
    let key = SymmetricKey([1_u8; 32]);
    let nonce = Nonce([2_u8; 24]);
    let file_id = FileId([0_u8; 32]);
    let aad = build_chunk_aad(file_id, 0, 0);

    for len in 0..16 {
        let short_data = vec![0_u8; len];
        let result = decrypt_chunk(&key, nonce, &short_data, &aad);

        assert!(
            result.is_err(),
            "short ciphertext should return Err: len={len}"
        );
    }
}
