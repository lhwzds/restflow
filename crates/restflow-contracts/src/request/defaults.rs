const DEFAULT_BACKGROUND_MAX_TOOL_CALLS: usize = 100;
const DEFAULT_AGENT_TASK_TIMEOUT_SECS: u64 = 1800;
const DEFAULT_AGENT_MAX_DURATION_SECS: u64 = 1800;

pub fn default_true() -> bool {
    true
}

pub fn default_cli_timeout_secs() -> u64 {
    DEFAULT_AGENT_TASK_TIMEOUT_SECS
}

pub fn default_memory_max_messages() -> usize {
    100
}

pub fn default_memory_scope() -> super::MemoryScope {
    super::MemoryScope::SharedAgent
}

pub fn default_memory_compaction_enabled() -> bool {
    true
}

pub fn default_memory_compaction_threshold_ratio() -> f32 {
    0.80
}

pub fn default_memory_max_summary_tokens() -> usize {
    2_000
}

pub fn default_max_tool_calls() -> usize {
    DEFAULT_BACKGROUND_MAX_TOOL_CALLS
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

pub fn default_memory_limit() -> u32 {
    50
}
