use std::time::Instant;

use etle::crypto::key_exchange::{AuthTag, auth_tags_equal};

#[test]
fn auth_tags_equal_is_constant_time() {
    let base: AuthTag = [0_u8; 32];
    let wrong_first_byte: AuthTag = {
        let mut tag = [0_u8; 32];
        tag[0] = 1;
        tag
    };
    let wrong_last_byte: AuthTag = {
        let mut tag = [0_u8; 32];
        tag[31] = 1;
        tag
    };

    let runs = 10_000;

    let start = Instant::now();
    for _ in 0..runs {
        let _ = auth_tags_equal(&base, &wrong_first_byte);
    }
    let time_first = start.elapsed();

    let start = Instant::now();
    for _ in 0..runs {
        let _ = auth_tags_equal(&base, &wrong_last_byte);
    }
    let time_last = start.elapsed();

    let diff = time_first.as_nanos().abs_diff(time_last.as_nanos());
    let threshold = time_first.as_nanos() / 2;

    assert!(
        diff < threshold,
        "timing delta too large: diff={diff}ns first={time_first:?} last={time_last:?}"
    );
}
