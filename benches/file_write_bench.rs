use criterion::{Criterion, criterion_group, criterion_main};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

fn write_direct(path: &Path, data: &[u8]) {
    fs::write(path, data).unwrap();
}

fn write_atomic(path: &Path, data: &[u8]) {
    let tmp = atomic_temp_path(path);
    let _ = fs::remove_file(&tmp);

    let mut file = fs::File::create(&tmp).unwrap();
    file.write_all(data).unwrap();
    file.sync_all().unwrap();
    drop(file);
    #[cfg(windows)]
    let _ = fs::remove_file(path);

    fs::rename(&tmp, path).unwrap();
}

fn atomic_temp_path(path: &Path) -> PathBuf {
    path.with_extension(format!("tmp-{}", std::process::id()))
}

fn bench_file_write(c: &mut Criterion) {
    let data = vec![0u8; 4096]; // ukuran tipikal state file
    let path = std::env::temp_dir().join(format!("etle_bench_write_{}.bin", std::process::id()));
    let mut group = c.benchmark_group("file_write");

    group.bench_function("fs_write_direct", |b| b.iter(|| write_direct(&path, &data)));

    group.bench_function("write_file_atomic", |b| {
        b.iter(|| write_atomic(&path, &data))
    });

    group.finish();

    // Bersihkan file sementara
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(atomic_temp_path(&path));
}

criterion_group!(benches, bench_file_write);
criterion_main!(benches);
