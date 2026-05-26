use criterion::{Criterion, criterion_group, criterion_main};
use std::path::PathBuf;

fn write_direct(path: &PathBuf, data: &[u8]) {
    std::fs::write(path, data).unwrap();
}

fn write_atomic(path: &PathBuf, data: &[u8]) {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).unwrap();
    // fsync simulasi (flush)
    let f = std::fs::File::open(&tmp).unwrap();
    f.sync_all().unwrap();
    std::fs::rename(&tmp, path).unwrap();
}

fn bench_file_write(c: &mut Criterion) {
    let data = vec![0u8; 4096]; // ukuran tipikal state file
    let path = std::env::temp_dir().join("etle_bench_write.bin");
    let mut group = c.benchmark_group("file_write");

    group.bench_function("fs_write_direct", |b| b.iter(|| write_direct(&path, &data)));

    group.bench_function("write_file_atomic", |b| {
        b.iter(|| write_atomic(&path, &data))
    });

    group.finish();

    // Bersihkan file sementara
    let _ = std::fs::remove_file(&path);
}

criterion_group!(benches, bench_file_write);
criterion_main!(benches);
