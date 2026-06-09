mod common;

use common::{hex_prefix, print_banner, print_kv, print_result, print_step};
use etle::crypto::{
    aead::{Nonce, SymmetricKey, build_chunk_aad, decrypt_chunk, encrypt_chunk},
    hash::FileId,
};

#[test]
fn demo_full_pipeline_encode_encrypt_decrypt_decode() {
    let message = "Hello";
    let plaintext = message.as_bytes().to_vec();

    let file_id_hash = blake3::hash(&plaintext);
    let file_id = FileId(*file_id_hash.as_bytes());

    let key = SymmetricKey([0x07_u8; 32]);
    let nonce = Nonce([0xa1_u8; 24]);
    let chunk_index = 0;
    let aad = build_chunk_aad(file_id, chunk_index, plaintext.len() as u64);

    let ciphertext = encrypt_chunk(&key, nonce, &plaintext, &aad).expect("encryption failed");
    let decrypted = decrypt_chunk(&key, nonce, &ciphertext, &aad).expect("decryption failed");
    let decoded = String::from_utf8(decrypted.clone()).expect("decrypted bytes should be UTF-8");

    let output_hash = blake3::hash(&decrypted);
    let verified = output_hash.as_bytes() == &file_id.0;

    print_banner("DEMO PIPELINE ETLE: Encode -> Encrypt -> Decrypt -> Decode");

    print_step(1, "encode input");
    print_kv("input_text", message);
    print_kv("input_bytes", format_args!("{:?}", plaintext));
    print_kv("input_hex", hex::encode(&plaintext));

    print_step(2, "BLAKE3 file identity");
    print_kv("file_id", hex::encode(file_id.0));
    print_kv("purpose", "file_id is bound into AEAD AAD");

    print_step(3, "cryptographic context");
    print_kv("key", "[0x07; 32] test key");
    print_kv("nonce", "[0xa1; 24] test nonce");
    print_kv("chunk_index", chunk_index);
    print_kv("plain_size", plaintext.len());
    print_kv("aad", hex::encode(&aad));

    print_step(4, "XChaCha20-Poly1305 encrypt");
    print_kv("plaintext_hex", hex::encode(&plaintext));
    print_kv("ciphertext", hex_prefix(&ciphertext, 16));
    print_kv("ciphertext_len", ciphertext.len());
    print_kv("auth_tag_len", 16);

    print_step(5, "transport frame simulation");
    print_kv("frame_layout", "ciphertext || poly1305_tag");
    print_kv("frame_len", ciphertext.len());
    print_kv("status", "sent_to_peer");

    print_step(6, "verify tag and decrypt");
    print_kv("tag_status", "valid");
    print_kv("decrypted_hex", hex::encode(&decrypted));
    print_kv("matches_plaintext", decrypted == plaintext);

    print_step(7, "decode plaintext");
    print_kv("decoded", &decoded);
    print_kv("matches_input_text", decoded == message);

    print_step(8, "final integrity check");
    print_kv("output_blake3", hex::encode(output_hash.as_bytes()));
    print_kv("expected_file_id", hex::encode(file_id.0));
    print_kv("verified", verified);

    print_result("pipeline_demo", "ok");

    assert_eq!(decoded, message);
    assert!(verified);
}
