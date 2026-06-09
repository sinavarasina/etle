#![allow(dead_code)]

pub fn print_banner(title: &str) {
    println!();
    println!("============================================");
    println!("{title}");
    println!("============================================");
}

pub fn print_step(index: usize, title: &str) {
    println!();
    println!("[ step={index} title=\"{title}\" ]");
}

pub fn print_kv(key: &str, value: impl std::fmt::Display) {
    println!("{key}={value}");
}

pub fn print_result(test: &str, status: &str) {
    println!();
    println!("result={status}");
    println!("test={test}");
}

pub fn hex_prefix(bytes: &[u8], max_len: usize) -> String {
    let shown = bytes.len().min(max_len);
    let mut output = String::with_capacity(shown * 2 + 3);

    for byte in bytes.iter().take(shown) {
        output.push_str(&format!("{byte:02x}"));
    }

    if bytes.len() > shown {
        output.push_str("...");
    }

    output
}
