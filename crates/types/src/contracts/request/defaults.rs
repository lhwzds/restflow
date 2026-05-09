const DEFAULT_TASK_MAX_TOOL_CALLS: usize = 100;
const DEFAULT_AGENT_MAX_DURATION_SECS: u64 = 1800;

pub fn default_max_tool_calls() -> usize {
    DEFAULT_TASK_MAX_TOOL_CALLS
}

pub fn default_max_duration_secs() -> u64 {
    DEFAULT_AGENT_MAX_DURATION_SECS
}

pub fn default_max_output_bytes() -> usize {
    1_000_000
}

pub fn default_segment_iterations() -> usize {
    50
}

pub fn default_max_total_iterations() -> usize {
    500
}

pub fn default_inter_segment_pause_ms() -> u64 {
    1_000
}
