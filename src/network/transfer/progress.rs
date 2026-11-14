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
    with_context_label_for_job(
        context, share_id, None, role, action, log_level, done, total, index, bytes,
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn init_context_session(
    context: &str,
    role: &str,
    action: &str,
    total: usize,
    initial_done: usize,
    baseline_bytes: u64,
    known_total_bytes: Option<u64>,
    explicit_job_id: Option<String>,
) {
    let key = ProgressKey::new(context.to_string(), role, action, total);
    let now = Instant::now();
    progress_states()
        .expect_lock("transfer progress state mutex poisoned")
        .insert(
            key,
            ProgressState::with_session(
                now,
                initial_done,
                baseline_bytes,
                known_total_bytes,
                explicit_job_id,
            ),
        );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn init_share_session(
    share_id: ShareId,
    role: &str,
    action: &str,
    total: usize,
    initial_done: usize,
    baseline_bytes: u64,
    known_total_bytes: Option<u64>,
    explicit_job_id: Option<String>,
) {
    init_context_session(
        &share_id.to_string(),
        role,
        action,
        total,
        initial_done,
        baseline_bytes,
        known_total_bytes,
        explicit_job_id,
    );
}

pub(super) struct ProgressSessionGuard {
    context: String,
    role: &'static str,
    action: &'static str,
    total: usize,
}

impl ProgressSessionGuard {
    pub(super) fn new(
        context: impl Into<String>,
        role: &'static str,
        action: &'static str,
        total: usize,
    ) -> Self {
        Self {
            context: context.into(),
            role,
            action,
            total,
        }
    }
}

impl Drop for ProgressSessionGuard {
    fn drop(&mut self) {
        clear_context_session(&self.context, self.role, self.action, self.total);
    }
}

pub(super) fn clear_context_session(context: &str, role: &str, action: &str, total: usize) {
    let key = ProgressKey::new(context.to_string(), role, action, total);
    progress_states()
        .expect_lock("transfer progress state mutex poisoned")
        .remove(&key);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_reused_snapshot(
    share_id: ShareId,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    bytes_done: u64,
    total_bytes: u64,
) {
    if done == 0 {
        return;
    }

    let job_id = super::jobs::active_job_id(share_id);
    if let Some(job_id) = &job_id {
        super::jobs::update_job_progress(
            job_id,
            "peer:reused",
            done,
            total,
            bytes_done,
            total_bytes,
            0,
        );
    }

    events::publish(IpcEvent::TransferProgress {
        job_id: job_id.clone(),
        share_id,
        completed_chunks: done,
        total_chunks: total,
        bytes_done,
        total_bytes,
        bytes_per_second: 0,
    });
    events::publish(IpcEvent::TaskProgress {
        job_id,
        task: "peer:reused".to_string(),
        label: share_id.to_string(),
        completed_chunks: done,
        total_chunks: total,
        bytes_done,
        total_bytes,
        bytes_per_second: 0,
    });

    if matches!(log_level, TransferLogLevel::Quiet) {
        return;
    }

    let percent = progress_percent_bytes(bytes_done, total_bytes);
    println!(
        "[peer] reused chunk {done}/{total} ({percent:.2}%) | progress={}/{} | avg=0 B/s | eta=--",
        format_bytes(bytes_done),
        format_bytes(total_bytes),
    );
}

#[allow(clippy::too_many_arguments)]
pub(super) fn with_context_label_for_job(
    context: String,
    share_id: Option<ShareId>,
    job_id_hint: Option<String>,
    role: &str,
    action: &str,
    log_level: TransferLogLevel,
    done: usize,
    total: usize,
    index: u32,
    bytes: usize,
) {
    let key = ProgressKey::new(context, role, action, total);
    let now = Instant::now();
    let snapshot = {
        let mut states = progress_states().expect_lock("transfer progress state mutex poisoned");
        let state = states
            .entry(key.clone())
            .or_insert_with(|| ProgressState::new(now));

        if let Some(job_id) = job_id_hint
            && state.explicit_job_id.is_none()
        {
            state.explicit_job_id = Some(job_id);
        }

        if done > state.last_done {
            state.last_done = done;
        }
        if state.counted_indices.insert(index) {
            state.session_bytes = state.session_bytes.saturating_add(bytes as u64);
        }

        let effective_done = state.last_done.max(done);
        let raw_done = state.baseline_bytes.saturating_add(state.session_bytes);
        let total_bytes = state
            .known_total_bytes
            .unwrap_or_else(|| estimated_total_bytes(raw_done, effective_done, total));
        let bytes_done = if effective_done >= total && total_bytes > 0 {
            total_bytes
        } else if total_bytes > 0 {
            raw_done.min(total_bytes)
        } else {
            raw_done
        };
        let average_rate = bytes_per_second(state.session_bytes, now.duration_since(state.start));
        let should_update_job = effective_done >= total
            || now.duration_since(state.last_job_update) >= Duration::from_millis(250);
        if should_update_job {
            state.last_job_update = now;
        }

        let should_emit = should_log_progress(
            log_level,
            effective_done,
            total,
            now.duration_since(state.last_log),
        );
        let line = if should_emit {
            state.last_log = now;
            Some(format_progress_line(
                role,
                action,
                effective_done,
                total,
                index,
                bytes,
                bytes_done,
                total_bytes,
                average_rate,
            ))
        } else {
            None
        };

        let explicit_job_id = state.explicit_job_id.clone();

        (
            effective_done,
            bytes_done,
            total_bytes,
            average_rate,
            should_update_job,
            explicit_job_id,
            line,
        )
    };

    let (
        effective_done,
        bytes_done,
        total_bytes,
        average_rate,
        should_update_job,
        explicit_job_id,
        line,
    ) = snapshot;

    let effective_job_id =
        explicit_job_id.or_else(|| share_id.and_then(super::jobs::active_job_id));
    let task = format!("{role}:{action}");

    if should_update_job {
        if let Some(job_id) = &effective_job_id {
            super::jobs::update_job_progress(
                job_id,
                &task,
                effective_done,
                total,
                bytes_done,
                total_bytes,
                average_rate as u64,
            );
        }

        if let Some(share_id) = share_id {
            if should_publish_legacy_transfer_progress(role, action) {
                events::publish(IpcEvent::TransferProgress {
                    job_id: effective_job_id.clone(),
                    share_id,
                    completed_chunks: effective_done,
                    total_chunks: total,
                    bytes_done,
                    total_bytes,
                    bytes_per_second: average_rate as u64,
                });
            }
            events::publish(IpcEvent::TaskProgress {
                job_id: effective_job_id,
                task,
                label: share_id.to_string(),
                completed_chunks: effective_done,
                total_chunks: total,
                bytes_done,
                total_bytes,
                bytes_per_second: average_rate as u64,
            });
        } else {
            events::publish(IpcEvent::TaskProgress {
                job_id: effective_job_id,
                task,
                label: key.context,
                completed_chunks: effective_done,
                total_chunks: total,
                bytes_done,
                total_bytes,
                bytes_per_second: average_rate as u64,
            });
        }
    }

    if let Some(line) = line {
        println!("{line}");
    }
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
    with_context_label_for_job(
        context, share_id, None, role, action, log_level, done, total, index, bytes,
    );
}

fn should_publish_legacy_transfer_progress(role: &str, action: &str) -> bool {
    role == "peer" && (action.contains("received+verified") || action == "reused")
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
    last_job_update: Instant,
    last_done: usize,
    baseline_bytes: u64,
    session_bytes: u64,
    known_total_bytes: Option<u64>,
    explicit_job_id: Option<String>,
    counted_indices: BTreeSet<u32>,
}

impl ProgressState {
    fn new(now: Instant) -> Self {
        Self::with_session(now, 0, 0, None, None)
    }

    fn with_session(
        now: Instant,
        initial_done: usize,
        baseline_bytes: u64,
        known_total_bytes: Option<u64>,
        explicit_job_id: Option<String>,
    ) -> Self {
        let initial_last = now.checked_sub(Duration::from_secs(1)).unwrap_or(now);
        Self {
            start: now,
            last_log: initial_last,
            last_job_update: initial_last,
            last_done: initial_done,
            baseline_bytes,
            session_bytes: 0,
            known_total_bytes,
            explicit_job_id,
            counted_indices: BTreeSet::new(),
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
    bytes_done: u64,
    total_bytes: u64,
    average_rate: f64,
) -> String {
    let percent = progress_percent_bytes(bytes_done, total_bytes);
    let eta = estimate_eta(bytes_done, total_bytes, average_rate);

    format!(
        "[{role}] {action} chunk {done}/{total} ({percent:.2}%) | \
        index={index} | chunk={} | progress={}/{} | avg={} | eta={}",
        format_bytes(bytes as u64),
        format_bytes(bytes_done),
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
    let seconds = remaining / average_rate;
    if !seconds.is_finite() {
        return None;
    }
    Duration::try_from_secs_f64(seconds).ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_guard_retains_state_on_completion_prevents_backward_done_and_cleans_on_drop() {
        let context = format!("test-guard-{}", std::process::id());
        let key = ProgressKey::new(context.clone(), "peer", "downloaded", 10);
        let guard = ProgressSessionGuard::new(context.clone(), "peer", "downloaded", 10);

        with_context_label_for_job(
            context.clone(),
            None,
            None,
            "peer",
            "downloaded",
            TransferLogLevel::Quiet,
            5,
            10,
            0,
            256,
        );
        with_context_label_for_job(
            context.clone(),
            None,
            None,
            "peer",
            "downloaded",
            TransferLogLevel::Quiet,
            2,
            10,
            1,
            256,
        );

        {
            let states = progress_states().expect_lock("progress states mutex");
            let state = states.get(&key).expect("session entry kept while active");
            assert_eq!(state.last_done, 5);
            assert_eq!(state.session_bytes, 512);
        }

        // Re-reporting an already-completed chunk at done == total must neither double-count nor
        // delete the active session before ProgressSessionGuard drops.
        with_context_label_for_job(
            context,
            None,
            None,
            "peer",
            "downloaded",
            TransferLogLevel::Quiet,
            10,
            10,
            1,
            256,
        );

        {
            let states = progress_states().expect_lock("progress states mutex");
            let state = states.get(&key).expect("entry retained until guard drop");
            assert_eq!(state.last_done, 10);
            assert_eq!(state.session_bytes, 512);
        }

        drop(guard);

        let states = progress_states().expect_lock("progress states mutex");
        assert!(!states.contains_key(&key));
    }

    #[test]
    fn estimate_eta_handles_extreme_or_invalid_inputs_without_panic() {
        assert_eq!(estimate_eta(0, u64::MAX, 1e-320), None);
        assert_eq!(estimate_eta(1024, 1024, 100.0), None);
        assert_eq!(estimate_eta(0, 1024, 0.0), None);
        assert_eq!(estimate_eta(0, 1024, f64::NAN), None);
        assert_eq!(
            estimate_eta(100, 1000, 200.0),
            Some(Duration::from_secs_f64(4.5))
        );
    }
}
