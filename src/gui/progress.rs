#[derive(Debug, Clone)]
pub struct TaskProgressSnapshot {
    pub job_id: Option<String>,
    pub task: String,
    pub label: String,
    pub completed_chunks: usize,
    pub total_chunks: usize,
    pub bytes_done: u64,
    pub total_bytes: u64,
    pub bytes_per_second: u64,
}
