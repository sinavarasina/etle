#[cfg(feature = "cli")]
fn main() -> anyhow::Result<()> {
    println!("etle p2p-cli: Sprint 1 core library scaffold is ready");
    Ok(())
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("p2p-cli requires the `cli` feature");
}
