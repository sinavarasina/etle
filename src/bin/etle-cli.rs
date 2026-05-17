#[cfg(feature = "cli")]
fn main() -> anyhow::Result<()> {
    println!("etle-cli: Sprint 1 core library scaffold is ready");
    Ok(())
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("etle-cli requires the `cli` feature");
}
