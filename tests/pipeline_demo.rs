use etle::crypto::aead::{Nonce, SymmetricKey, build_chunk_aad, decrypt_chunk, encrypt_chunk};
use etle::crypto::hash::FileId;

#[test]
fn demo_full_pipeline_encode_encrypt_decrypt_decode() {
    println!("\n========================================");
    println!("  DEMO PIPELINE ETLE: Encode → Decrypt → Decode");
    println!("========================================\n");

    // ── TAHAP 1: ENCODE ──────────────────────────
    let pesan = "Halo";
    let plaintext: Vec<u8> = pesan.as_bytes().to_vec();

    println!("[ TAHAP 1 - ENCODE ]");
    println!("  Input teks   : \"{}\"", pesan);
    print!("  Hasil encode : [");
    for (i, b) in plaintext.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("0x{:02X}", b);
    }
    println!("]");
    println!("  Keterangan   : H=0x48, a=0x61, l=0x6C, o=0x6F\n");

    // ── TAHAP 2: HASH BLAKE3 ─────────────────────
    let file_id_bytes = blake3::hash(&plaintext);
    let file_id = FileId(*file_id_bytes.as_bytes());

    println!("[ TAHAP 2 - HASH BLAKE3 ]");
    println!("  Input        : {:?}", plaintext);
    println!("  FileId       : {}", hex::encode(file_id.0));
    println!("  Keterangan   : FileId digunakan sebagai komponen AAD\n");

    // ── TAHAP 3: SIAPKAN KEY, NONCE, AAD ─────────
    let key = SymmetricKey([0x07u8; 32]);
    let nonce = Nonce([0xA1u8; 24]);
    let chunk_index: u32 = 0;
    let aad = build_chunk_aad(file_id, chunk_index, plaintext.len() as u64);

    println!("[ TAHAP 3 - PERSIAPAN KRIPTOGRAFI ]");
    println!("  Key (32 byte)  : [0x07 x 32] (disederhanakan)");
    println!("  Nonce (24 byte): [0xA1 x 24] (disederhanakan)");
    println!("  AAD            : FileId(32) || chunk_index(4) || plain_size(8)");
    println!("  AAD hex        : {}\n", hex::encode(&aad));

    // ── TAHAP 4: ENCRYPT ─────────────────────────
    let ciphertext = encrypt_chunk(&key, nonce, &plaintext, &aad).expect("enkripsi gagal");

    println!("[ TAHAP 4 - ENCRYPT XChaCha20-Poly1305 ]");
    println!("  Plaintext  : {}", hex::encode(&plaintext));
    println!(
        "  Ciphertext : {}",
        hex::encode(&ciphertext[..plaintext.len()])
    );
    println!(
        "  Auth Tag   : {}",
        hex::encode(&ciphertext[plaintext.len()..])
    );
    println!(
        "  Total output: {} byte (plaintext + 16 byte tag)\n",
        ciphertext.len()
    );

    // ── TAHAP 5: SIMULASI TRANSFER ───────────────
    println!("[ TAHAP 5 - TRANSFER (SIMULASI JARINGAN) ]");
    let frame_len = ciphertext.len() as u32;
    println!("  Length prefix  : {} byte (4 byte big-endian)", frame_len);
    println!(
        "  Frame dikirim  : [length(4)] + [ciphertext({} byte)]",
        ciphertext.len()
    );
    println!("  Status         : ✓ Data berhasil dikirim ke peer\n");

    // ── TAHAP 6: VERIFY + DECRYPT ────────────────
    println!("[ TAHAP 6 - VERIFY POLY1305 + DECRYPT ]");
    println!("  Verifikasi tag : ...");

    let decrypted = decrypt_chunk(&key, nonce, &ciphertext, &aad).expect("dekripsi gagal");

    println!("  Tag valid      : ✓ Poly1305 cocok");
    println!("  Hasil decrypt  : {}", hex::encode(&decrypted));
    println!(
        "  Sama dengan plaintext asli: {}\n",
        if decrypted == plaintext {
            "✓ YA"
        } else {
            "✗ TIDAK"
        }
    );

    // ── TAHAP 7: DECODE ──────────────────────────
    let hasil_decode = String::from_utf8(decrypted.clone()).expect("bukan UTF-8 valid");

    println!("[ TAHAP 7 - DECODE ]");
    print!("  Input bytes  : [");
    for (i, b) in decrypted.iter().enumerate() {
        if i > 0 {
            print!(", ");
        }
        print!("0x{:02X}", b);
    }
    println!("]");
    println!("  Hasil decode : \"{}\"", hasil_decode);
    println!(
        "  Sama dengan input awal: {}\n",
        if hasil_decode == pesan {
            "✓ YA"
        } else {
            "✗ TIDAK"
        }
    );

    // ── TAHAP 8: VERIFIKASI AKHIR BLAKE3 ────────
    let hash_output = blake3::hash(&decrypted);
    let cocok = hash_output.as_bytes() == &file_id.0;

    println!("[ TAHAP 8 - VERIFIKASI AKHIR BLAKE3 ]");
    println!("  BLAKE3(output) : {}", hex::encode(hash_output.as_bytes()));
    println!("  FileId seeder  : {}", hex::encode(file_id.0));
    println!(
        "  Hasil verifikasi: {}",
        if cocok {
            "✓ COCOK — File dinyatakan UTUH dan AUTENTIK"
        } else {
            "✗ TIDAK COCOK — File DIMANIPULASI"
        }
    );

    println!("\n========================================");
    println!("  PIPELINE SELESAI: Semua tahap berhasil");
    println!("========================================\n");

    assert_eq!(hasil_decode, pesan);
    assert!(cocok);
}
