use etle::crypto::key_exchange::{AuthTag, auth_tags_equal};
use std::time::Instant;

#[test]
fn auth_tags_equal_is_constant_time() {
    let base: AuthTag = [0u8; 32];
    let wrong_first_byte: AuthTag = {
        let mut t = [0u8; 32];
        t[0] = 1; // berbeda di byte pertama
        t
    };
    let wrong_last_byte: AuthTag = {
        let mut t = [0u8; 32];
        t[31] = 1; // berbeda di byte terakhir
        t
    };

    let runs = 10_000;

    // Ukur waktu perbandingan salah di byte pertama
    let start = Instant::now();
    for _ in 0..runs {
        let _ = auth_tags_equal(&base, &wrong_first_byte);
    }
    let time_first = start.elapsed();

    // Ukur waktu perbandingan salah di byte terakhir
    let start = Instant::now();
    for _ in 0..runs {
        let _ = auth_tags_equal(&base, &wrong_last_byte);
    }
    let time_last = start.elapsed();

    println!("Waktu salah di byte pertama : {:?}", time_first);
    println!("Waktu salah di byte terakhir: {:?}", time_last);

    // Selisih harus sangat kecil (tidak short-circuit)
    let diff = time_first.as_nanos().abs_diff(time_last.as_nanos());
    let threshold = time_first.as_nanos() / 2; // toleransi 50%

    assert!(
        diff < threshold,
        "Selisih terlalu besar: {}ns — kemungkinan tidak constant-time",
        diff
    );
}
