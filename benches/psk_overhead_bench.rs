use criterion::{Criterion, criterion_group, criterion_main};
use etle::crypto::key_exchange::AuthPsk;
use etle::network::key_exchange::{
    client_authenticated_session_key_exchange, client_session_key_exchange,
    server_authenticated_session_key_exchange, server_session_key_exchange,
};
use tokio::runtime::Runtime;

fn bench_key_exchange(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("crypto.key_exchange");

    group.bench_function("unauthenticated", |b| {
        b.to_async(&rt).iter(|| async {
            let (mut client_stream, mut server_stream) = tokio::io::duplex(4096);
            tokio::join!(
                client_session_key_exchange(&mut client_stream),
                server_session_key_exchange(&mut server_stream)
            )
        })
    });

    group.bench_function("authenticated_psk", |b| {
        let psk = AuthPsk::from_passphrase("benchmark-passphrase");
        b.to_async(&rt).iter(|| async {
            let (mut client_stream, mut server_stream) = tokio::io::duplex(4096);
            let server_psk = psk.clone();

            tokio::join!(
                client_authenticated_session_key_exchange(&mut client_stream, &psk),
                server_authenticated_session_key_exchange(&mut server_stream, &server_psk)
            )
        })
    });

    group.finish();
}

criterion_group!(benches, bench_key_exchange);
criterion_main!(benches);
