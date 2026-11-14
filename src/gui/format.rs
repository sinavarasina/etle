use std::{
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

pub fn fraction(done: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0)
    }
}

pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;

    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }

    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

pub fn eta_text(bytes_done: u64, total_bytes: u64, bytes_per_second: u64) -> Option<String> {
    if bytes_per_second == 0 || total_bytes == 0 || bytes_done >= total_bytes {
        return None;
    }
    let remaining = total_bytes.saturating_sub(bytes_done);
    let secs = remaining.div_ceil(bytes_per_second);
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        Some(format!("{hours}h {minutes:02}m"))
    } else if minutes > 0 {
        Some(format!("{minutes}m {seconds:02}s"))
    } else {
        Some(format!("{seconds}s"))
    }
}

pub fn share_mode_display(
    mode: Option<&str>,
    is_downloading_active: bool,
    is_complete: bool,
) -> (&'static str, &'static str) {
    match mode {
        Some("seeding") => ("Seeding", "etle-badge-ok"),
        Some("completed") => ("Completed", "etle-badge-ok"),
        Some("downloading") => {
            if is_downloading_active {
                ("Downloading", "etle-badge-accent")
            } else if is_complete {
                ("Ready", "etle-badge-ok")
            } else {
                ("Partial (Paused)", "etle-badge-warn")
            }
        }
        Some("paused") => ("Paused", "etle-badge-warn"),
        _ => ("Unknown", "etle-badge-muted"),
    }
}

pub fn file_label(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn trim_path_label(value: &str) -> String {
    file_label(&PathBuf::from(value))
}

pub fn compact_log_line(value: impl AsRef<str>) -> String {
    value
        .as_ref()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn short_time() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() % 86_400)
        .unwrap_or(0);
    let hour = secs / 3600;
    let minute = (secs % 3600) / 60;
    let second = secs % 60;
    format!("{hour:02}:{minute:02}:{second:02}")
}
