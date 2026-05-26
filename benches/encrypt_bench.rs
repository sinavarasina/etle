use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use etle::crypto::{
    aead::{build_chunk_aad, encrypt_chunk, generate_nonce},
    hash::FileId,
    key_wrap::generate_file_key,
};

fn bench_encrypt_chunk(c: &mut Criterion) {
    let key = generate_file_key();
    let nonce = generate_nonce();
    let file_id = FileId([9u8; 32]);

    let mut group = c.benchmark_group("encrypt_chunk");

    // Uji tiga ukuran chunk berbeda
    for size in [64 * 1024, 512 * 1024, 1024 * 1024] {
        let data = vec![0u8; size];
        let aad = build_chunk_aad(file_id, 0, data.len() as u64);

        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(format!("{}KB", size / 1024)),
            &size,
            |b, _| b.iter(|| encrypt_chunk(&key, nonce, &data, &aad).unwrap()),
        );
    }
    group.finish();
}

criterion_group!(benches, bench_encrypt_chunk);
criterion_main!(benches);
