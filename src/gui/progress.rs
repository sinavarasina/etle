#[derive(Debug)]
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

pub fn parse_task_progress_debug(value: &str) -> Option<TaskProgressSnapshot> {
    if !value.contains("TaskProgress") {
        return None;
    }

    let job_id = extract_debug_option_string(value, "job_id")
        .or_else(|| extract_debug_option_string(value, "job"));
    let task = extract_debug_string(value, "task")
        .or_else(|| extract_debug_string(value, "stage"))
        .unwrap_or_else(|| "task".to_string());
    let label = extract_debug_string(value, "label")
        .or_else(|| extract_debug_string(value, "path"))
        .unwrap_or_else(|| task.clone());
    let completed_chunks = extract_debug_usize(value, "completed_chunks")
        .or_else(|| extract_debug_usize(value, "chunks_done"))
        .or_else(|| extract_debug_usize(value, "done_chunks"))
        .unwrap_or(0);
    let total_chunks = extract_debug_usize(value, "total_chunks")
        .or_else(|| extract_debug_usize(value, "chunks_total"))
        .unwrap_or(0);
    let bytes_done = extract_debug_u64(value, "bytes_done")
        .or_else(|| extract_debug_u64(value, "done_bytes"))
        .unwrap_or(0);
    let total_bytes = extract_debug_u64(value, "total_bytes")
        .or_else(|| extract_debug_u64(value, "bytes_total"))
        .unwrap_or(0);
    let bytes_per_second = extract_debug_u64(value, "bytes_per_second")
        .or_else(|| extract_debug_u64(value, "speed"))
        .unwrap_or(0);

    Some(TaskProgressSnapshot {
        job_id,
        task,
        label,
        completed_chunks,
        total_chunks,
        bytes_done,
        total_bytes,
        bytes_per_second,
    })
}

fn extract_debug_string(value: &str, field: &str) -> Option<String> {
    let marker = format!("{field}: ");
    let start = value.find(&marker)? + marker.len();
    let rest = &value[start..];
    let quote_start = rest.find('"')? + 1;
    let after_quote = &rest[quote_start..];
    let quote_end = after_quote.find('"')?;
    Some(after_quote[..quote_end].to_string())
}

fn extract_debug_option_string(value: &str, field: &str) -> Option<String> {
    let marker = format!("{field}: ");
    let start = value.find(&marker)? + marker.len();
    let rest = &value[start..];
    if rest.starts_with("None") {
        return None;
    }
    extract_debug_string(value, field)
}

fn extract_debug_usize(value: &str, field: &str) -> Option<usize> {
    extract_debug_u64(value, field).and_then(|number| usize::try_from(number).ok())
}

fn extract_debug_u64(value: &str, field: &str) -> Option<u64> {
    let marker = format!("{field}: ");
    let start = value.find(&marker)? + marker.len();
    let rest = &value[start..];
    let digits = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}
