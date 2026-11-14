use super::prelude::*;

#[derive(Default)]
struct JobRegistry {
    by_job_id: BTreeMap<String, IpcActiveJob>,
    by_share_id: BTreeMap<ShareId, String>,
}

/// Associates a share with an active CLI/daemon transfer job identifier.
pub fn register(share_id: ShareId, job_id: impl Into<String>) {
    registry()
        .expect_lock("transfer job registry mutex poisoned")
        .by_share_id
        .insert(share_id, job_id.into());
}

/// Removes a share's active job association if it still matches `job_id`.
pub fn unregister(share_id: ShareId, job_id: &str) {
    let mut guard = registry().expect_lock("transfer job registry mutex poisoned");
    if guard
        .by_share_id
        .get(&share_id)
        .is_some_and(|active| active == job_id)
    {
        guard.by_share_id.remove(&share_id);
    }
}

/// Starts tracking an active job in the daemon registry and broadcasts `IpcEvent::JobStarted`.
pub fn start_job(job: IpcActiveJob) {
    {
        let mut guard = registry().expect_lock("transfer job registry mutex poisoned");
        if job.kind != IpcJobKind::Upload
            && let Some(share_id) = job.share_id
        {
            guard.by_share_id.insert(share_id, job.job_id.clone());
        }
        guard.by_job_id.insert(job.job_id.clone(), job.clone());
    }
    events::publish(IpcEvent::JobStarted { job });
}

/// Atomically checks for an existing active job of the same kind and share ID, inserting `job`
/// and emitting `IpcEvent::JobStarted` only when no conflicting job is running.
pub fn try_start_job(job: IpcActiveJob) -> bool {
    {
        let mut guard = registry().expect_lock("transfer job registry mutex poisoned");
        if guard.by_job_id.contains_key(&job.job_id) {
            return false;
        }
        if let Some(share_id) = job.share_id
            && guard
                .by_job_id
                .values()
                .any(|existing| existing.kind == job.kind && existing.share_id == Some(share_id))
        {
            return false;
        }
        if job.kind != IpcJobKind::Upload
            && let Some(share_id) = job.share_id
        {
            guard.by_share_id.insert(share_id, job.job_id.clone());
        }
        guard.by_job_id.insert(job.job_id.clone(), job.clone());
    }
    events::publish(IpcEvent::JobStarted { job });
    true
}

/// Updates metadata (resolved share ID, display label, or totals) for a running job.
pub fn update_job_metadata(
    job_id: &str,
    share_id: Option<ShareId>,
    label: Option<String>,
    total_chunks: Option<usize>,
    total_bytes: Option<u64>,
) {
    let mut guard = registry().expect_lock("transfer job registry mutex poisoned");
    let mut link_share = None;
    if let Some(job) = guard.by_job_id.get_mut(job_id) {
        if let Some(share_id) = share_id {
            job.share_id = Some(share_id);
            if job.kind != IpcJobKind::Upload {
                link_share = Some((share_id, job.job_id.clone()));
            }
        }
        if let Some(label) = label {
            job.label = label;
        }
        if let Some(total_chunks) = total_chunks
            && total_chunks > 0
        {
            job.total_chunks = total_chunks;
        }
        if let Some(total_bytes) = total_bytes
            && total_bytes > 0
        {
            job.total_bytes = total_bytes;
        }
    }
    if let Some((share_id, job_id)) = link_share {
        guard.by_share_id.insert(share_id, job_id);
    }
}

pub(super) fn update_job_progress(
    job_id: &str,
    phase: &str,
    completed_chunks: usize,
    total_chunks: usize,
    bytes_done: u64,
    total_bytes: u64,
    bytes_per_second: u64,
) {
    let mut guard = registry().expect_lock("transfer job registry mutex poisoned");
    if let Some(job) = guard.by_job_id.get_mut(job_id) {
        job.phase = phase.to_string();
        job.completed_chunks = completed_chunks;
        if total_chunks > 0 {
            job.total_chunks = total_chunks;
        }
        job.bytes_done = bytes_done;
        if total_bytes > 0 {
            job.total_bytes = total_bytes;
        }
        job.bytes_per_second = bytes_per_second;
    }
}

/// Marks an active job as completed, removes it from active status, and broadcasts `IpcEvent::JobCompleted`.
pub fn complete_job(job_id: &str, summary: impl Into<String>) {
    if let Some(job) = remove_job(job_id) {
        events::publish(IpcEvent::JobCompleted {
            job_id: job.job_id,
            kind: job.kind,
            share_id: job.share_id,
            label: job.label,
            summary: summary.into(),
        });
    }
}

/// Marks an active job as failed, removes it from active status, and broadcasts `IpcEvent::JobFailed`.
pub fn fail_job(job_id: &str, error: impl Into<String>) {
    if let Some(job) = remove_job(job_id) {
        events::publish(IpcEvent::JobFailed {
            job_id: job.job_id,
            kind: job.kind,
            share_id: job.share_id,
            label: job.label,
            error: error.into(),
        });
    }
}

/// Returns a sorted snapshot of all active jobs currently running in the daemon.
pub fn active_job_list() -> Vec<IpcActiveJob> {
    registry()
        .expect_lock("transfer job registry mutex poisoned")
        .by_job_id
        .values()
        .cloned()
        .collect()
}

/// Returns true if a job of `kind` is currently active for `share_id`.
pub fn has_active_job(kind: IpcJobKind, share_id: ShareId) -> bool {
    registry()
        .expect_lock("transfer job registry mutex poisoned")
        .by_job_id
        .values()
        .any(|job| job.kind == kind && job.share_id == Some(share_id))
}

pub(super) fn active_job_id(share_id: ShareId) -> Option<String> {
    registry()
        .expect_lock("transfer job registry mutex poisoned")
        .by_share_id
        .get(&share_id)
        .cloned()
}

fn remove_job(job_id: &str) -> Option<IpcActiveJob> {
    let mut guard = registry().expect_lock("transfer job registry mutex poisoned");
    let job = guard.by_job_id.remove(job_id)?;
    guard.by_share_id.retain(|_, active| active != job_id);
    Some(job)
}

fn registry() -> &'static Mutex<JobRegistry> {
    static JOBS: OnceLock<Mutex<JobRegistry>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(JobRegistry::default()))
}
