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

    println!("test=pipeline_demo");
    println!("status=ok");
    println!("message={message}");
    println!("plaintext={}", hex::encode(&plaintext));
    println!("file_id={}", hex::encode(file_id.0));
    println!("aad={}", hex::encode(&aad));
    println!("ciphertext_len={}", ciphertext.len());
    println!("decoded={decoded}");
    println!("verified={verified}");

    assert_eq!(decoded, message);
    assert!(verified);
}
