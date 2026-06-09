mod common;

use common::{print_banner, print_kv, print_result, print_step};
use etle::crypto::{
    aead::{Nonce, SymmetricKey, build_chunk_aad, decrypt_chunk},
    hash::FileId,
};

#[test]
fn fuzz_decrypt_with_random_inputs() {
    print_banner("fuzz_decrypt_with_random_inputs");

    print_step(1, "try deterministic byte-pattern inputs");
    let mut cases = 0usize;
    for value in 0_u8..=255 {
        let key = SymmetricKey([value; 32]);
        let nonce = Nonce([value; 24]);
        let file_id = FileId([0_u8; 32]);
        let aad = build_chunk_aad(file_id, 0, 0);

        let _ = decrypt_chunk(&key, nonce, &[], &aad);
        cases += 1;
        let _ = decrypt_chunk(&key, nonce, &[value, value, value], &aad);
        cases += 1;

        let data = vec![value; 1024];
        let _ = decrypt_chunk(&key, nonce, &data, &aad);
        cases += 1;
    }
    print_kv("byte_values", 256);
    print_kv("decrypt_attempts", cases);
    print_kv("panic_free", true);

    print_result("fuzz_decrypt_with_random_inputs", "ok");
}

#[test]
fn fuzz_decrypt_truncated_tag() {
    print_banner("fuzz_decrypt_truncated_tag");

    print_step(1, "try ciphertext shorter than Poly1305 tag");
    let key = SymmetricKey([1_u8; 32]);
    let nonce = Nonce([2_u8; 24]);
    let file_id = FileId([0_u8; 32]);
    let aad = build_chunk_aad(file_id, 0, 0);

    for len in 0..16 {
        let short_data = vec![0_u8; len];
        let result = decrypt_chunk(&key, nonce, &short_data, &aad);
        print_kv(&format!("len_{len}_rejected"), result.is_err());
        assert!(
            result.is_err(),
            "short ciphertext should return Err: len={len}"
        );
    }

    print_result("fuzz_decrypt_truncated_tag", "ok");
}
