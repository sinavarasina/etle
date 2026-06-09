mod common;

use std::time::Instant;

use common::{print_banner, print_kv, print_result, print_step};
use etle::crypto::key_exchange::{AuthTag, auth_tags_equal};

#[test]
fn auth_tags_equal_is_constant_time() {
    print_banner("auth_tags_equal_is_constant_time");

    print_step(1, "prepare tags that differ at different positions");
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
    print_kv("runs", runs);

    print_step(2, "measure mismatch at first byte");
    let start = Instant::now();
    for _ in 0..runs {
        let _ = auth_tags_equal(&base, &wrong_first_byte);
    }
    let time_first = start.elapsed();
    print_kv("time_first", format_args!("{time_first:?}"));

    print_step(3, "measure mismatch at last byte");
    let start = Instant::now();
    for _ in 0..runs {
        let _ = auth_tags_equal(&base, &wrong_last_byte);
    }
    let time_last = start.elapsed();
    print_kv("time_last", format_args!("{time_last:?}"));

    print_step(4, "compare timing delta against tolerance");
    let diff = time_first.as_nanos().abs_diff(time_last.as_nanos());
    let threshold = time_first.as_nanos() / 2;
    print_kv("diff_ns", diff);
    print_kv("threshold_ns", threshold);
    print_kv("within_threshold", diff < threshold);

    assert!(
        diff < threshold,
        "timing delta too large: diff={diff}ns first={time_first:?} last={time_last:?}"
    );
    print_result("auth_tags_equal_is_constant_time", "ok");
}
