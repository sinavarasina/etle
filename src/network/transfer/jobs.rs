use super::prelude::*;

pub fn register(share_id: ShareId, job_id: impl Into<String>) {
    active_jobs()
        .lock()
        .expect("transfer job registry mutex poisoned")
        .insert(share_id, job_id.into());
}

pub fn unregister(share_id: ShareId, job_id: &str) {
    let mut jobs = active_jobs()
        .lock()
        .expect("transfer job registry mutex poisoned");

    if jobs.get(&share_id).is_some_and(|active| active == job_id) {
        jobs.remove(&share_id);
    }
}

pub(super) fn active_job_id(share_id: ShareId) -> Option<String> {
    active_jobs()
        .lock()
        .expect("transfer job registry mutex poisoned")
        .get(&share_id)
        .cloned()
}

fn active_jobs() -> &'static Mutex<BTreeMap<ShareId, String>> {
    static JOBS: OnceLock<Mutex<BTreeMap<ShareId, String>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(BTreeMap::new()))
}
