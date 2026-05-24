use super::prelude::*;

pub fn log(
    role: &str,
    action: &str,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
) {
    with_context(None, role, action, log_level, done, total, index, bytes);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn for_share(
    share_id: ShareId,
    role: &str,
    action: &str,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
) {
    with_context(
        Some(share_id),
        role,
        action,
        log_level,
        done,
        total,
        index,
        bytes,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn with_label(
    context: &str,
    role: &str,
    action: &str,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
) {
    with_context_label(
        context.to_string(),
        None,
        role,
        action,
        log_level,
        done,
        total,
        index,
        bytes,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn with_context(
    share_id: Option<ShareId>,
    role: &str,
    action: &str,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
) {
    let context = share_id
        .map(|share_id| share_id.to_string())
        .unwrap_or_else(|| "global".to_string());
    with_context_label(
        context, share_id, role, action, log_level, done, total, index, bytes,
    );
}

#[allow(clippy::too_many_arguments)]
fn with_context_label(
    context: String,
    share_id: Option<ShareId>,
    role: &str,
    action: &str,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
) {
    if matches!(log_level, TransferLogLevel::Quiet) {
        return;
    }

    let key = ProgressKey::new(context, role, action, total);
    let mut states = progress_states()
        .lock()
        .expect("transfer progress state mutex poisoned");
    let now = Instant::now();
    let state = states
        .entry(key.clone())
        .or_insert_with(|| ProgressState::new(now));

    if done > state.last_done {
        state.bytes_done = state.bytes_done.saturating_add(bytes as u64);
        state.last_done = done;
    }

    if !should_log_progress(log_level, done, total, now.duration_since(state.last_log)) {
        return;
    }

    let total_bytes = estimated_total_bytes(state.bytes_done, done, total);
    let average_rate = bytes_per_second(state.bytes_done, now.duration_since(state.start));
    let line = format_progress_line(
        role,
        action,
        done,
        total,
        index,
        bytes,
        state,
        total_bytes,
        average_rate,
    );
    state.last_log = now;

    if let Some(share_id) = share_id {
        events::publish(IpcEvent::TransferProgress {
            job_id: super::jobs::active_job_id(share_id),
            share_id,
            completed_chunks: done,
            total_chunks: total,
            bytes_done: state.bytes_done,
            total_bytes,
            bytes_per_second: average_rate as u64,
        });
    } else {
        events::publish(IpcEvent::TaskProgress {
            job_id: None,
            task: format!("{role}:{action}"),
            label: key.context.clone(),
            completed_chunks: done,
            total_chunks: total,
            bytes_done: state.bytes_done,
            total_bytes,
            bytes_per_second: average_rate as u64,
        });
    }

    if done >= total {
        states.remove(&key);
    }

    println!("{line}");
}

fn progress_states() -> &'static Mutex<BTreeMap<ProgressKey, ProgressState>> {
    static STATES: OnceLock<Mutex<BTreeMap<ProgressKey, ProgressState>>> = OnceLock::new();
    STATES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ProgressKey {
    context: String,
    role: String,
    action: String,
    total: usize,
}

impl ProgressKey {
    fn new(context: String, role: &str, action: &str, total: usize) -> Self {
        Self {
            context,
            role: role.to_string(),
            action: action.to_string(),
            total,
        }
    }
}

struct ProgressState {
    start: Instant,
    last_log: Instant,
    last_done: usize,
    bytes_done: u64,
}

impl ProgressState {
    fn new(now: Instant) -> Self {
        Self {
            start: now,
            last_log: now.checked_sub(Duration::from_secs(1)).unwrap_or(now),
            last_done: 0,
            bytes_done: 0,
        }
    }
}

fn should_log_progress(
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    since_last_log: Duration,
) -> bool {
    match log_level {
        TransferLogLevel::Quiet => false,
        TransferLogLevel::Verbose => true,
        TransferLogLevel::Normal => {
            done == 1 || done >= total || since_last_log >= Duration::from_millis(750)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn format_progress_line(
    role: &str,
    action: &str,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
    state: &ProgressState,
    total_bytes: u64,
    average_rate: f64,
) -> String {
    let percent = progress_percent_bytes(state.bytes_done, total_bytes);
    let eta = estimate_eta(state.bytes_done, total_bytes, average_rate);

    format!(
        "[{role}] {action} chunk {done}/{total} ({percent:.2}%) | \
        index={index} | chunk={} | progress={}/{} | avg={} | eta={}",
        format_bytes(bytes as u64),
        format_bytes(state.bytes_done),
        format_bytes(total_bytes),
        format_rate(average_rate),
        format_duration(eta),
    )
}

fn estimated_total_bytes(done_bytes: u64, done: usize, total: usize) -> u64 {
    if total == 0 {
        return 0;
    }

    if done >= total {
        return done_bytes;
    }

    if done == 0 || done_bytes == 0 {
        return 0;
    }

    let average_chunk = (done_bytes / done as u64).max(1);
    average_chunk.saturating_mul(total as u64)
}

fn progress_percent_bytes(done_bytes: u64, total_bytes: u64) -> f64 {
    if total_bytes == 0 {
        return 100.0;
    }

    (done_bytes as f64 * 100.0 / total_bytes as f64).min(100.0)
}

fn bytes_per_second(done_bytes: u64, elapsed: Duration) -> f64 {
    let secs = elapsed.as_secs_f64();
    if secs <= f64::EPSILON {
        return 0.0;
    }

    done_bytes as f64 / secs
}

fn estimate_eta(done_bytes: u64, total_bytes: u64, average_rate: f64) -> Option<Duration> {
    if done_bytes >= total_bytes || average_rate <= f64::EPSILON {
        return None;
    }

    let remaining = total_bytes.saturating_sub(done_bytes) as f64;
    Some(Duration::from_secs_f64(remaining / average_rate))
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = UNITS[0];

    for candidate in &UNITS[1..] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = candidate;
    }

    if unit == "B" {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {unit}")
    }
}

fn format_rate(bytes_per_second: f64) -> String {
    if bytes_per_second <= f64::EPSILON {
        return "0 B/s".to_string();
    }

    format!("{}/s", format_bytes(bytes_per_second as u64))
}

fn format_duration(duration: Option<Duration>) -> String {
    let Some(duration) = duration else {
        return "--".to_string();
    };

    let seconds = duration.as_secs();
    let minutes = seconds / 60;
    let seconds = seconds % 60;

    if minutes == 0 {
        format!("{seconds}s")
    } else {
        format!("{minutes}m {seconds}s")
    }
}
