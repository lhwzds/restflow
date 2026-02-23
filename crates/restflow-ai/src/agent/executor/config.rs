use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::agent::PromptFlags;
use crate::agent::context::AgentContext;
use crate::agent::model_router::{ModelRoutingConfig, ModelSwitcher};
use crate::agent::resource::{ResourceLimits, ResourceUsage};
use crate::agent::scratchpad::Scratchpad;
use crate::agent::state::AgentState;
use crate::agent::stuck::StuckDetectorConfig;
use crate::error::Result;

/// Default maximum number of tool calls that can execute concurrently.
pub const DEFAULT_MAX_TOOL_CONCURRENCY: usize = 100;

pub const MAX_TOOL_RETRIES: usize = 2;

/// Persistence frequency for execution checkpoints.
#[derive(Debug, Clone)]
pub enum CheckpointDurability {
    /// Persist state after each ReAct turn.
    PerTurn,
    /// Persist state every N turns.
    Periodic { interval: usize },
    /// Persist state only on terminal completion/failure.
    OnComplete,
}

impl Default for CheckpointDurability {
    fn default() -> Self {
        Self::Periodic { interval: 5 }
    }
}

pub type CheckpointFuture = Pin<Box<dyn Future<Output = Result<()>> + Send + 'static>>;
pub type CheckpointCallback = Arc<dyn Fn(&AgentState) -> CheckpointFuture + Send + Sync>;

/// Configuration for agent execution
#[derive(Clone)]
pub struct AgentConfig {
    pub goal: String,
    pub system_prompt: Option<String>,
    pub max_iterations: usize,
    pub temperature: Option<f32>,
    /// Hidden context passed to tools but not shown to LLM (Swarm-inspired)
    pub context: HashMap<String, Value>,
    /// Timeout for each tool execution (default: 300s).
    ///
    /// This is the **wrapper timeout** applied by the executor. To avoid confusing
    /// errors, this should be >= the tool-internal timeout (e.g., `bash_timeout_secs`)
    /// plus a small buffer. See module-level docs for details.
    pub tool_timeout: Duration,
    /// Max length for tool results to prevent context overflow (default: 4000)
    pub max_tool_result_length: usize,
    /// Context window size in tokens (default: 128000).
    pub context_window: usize,
    /// Optional maximum output tokens for each LLM completion request.
    pub max_output_tokens: Option<u32>,
    /// Optional agent context injected into the system prompt.
    pub agent_context: Option<AgentContext>,
    /// Whether to inject agent_context into system prompt (default: true).
    pub inject_agent_context: bool,
    /// Resource limits for guardrails (tool calls, wall-clock, depth).
    pub resource_limits: ResourceLimits,
    /// Optional stuck detection configuration.
    /// When enabled, detects when the agent repeatedly calls the same tool
    /// with the same arguments and either nudges or stops execution.
    pub stuck_detection: Option<StuckDetectorConfig>,
    /// Optional append-only JSONL scratchpad for execution diagnostics.
    pub scratchpad: Option<Arc<Scratchpad>>,
    /// Optional model routing configuration for dynamic tier-based switching.
    pub model_routing: Option<ModelRoutingConfig>,
    /// Optional model switcher used when model routing is enabled.
    pub model_switcher: Option<Arc<dyn ModelSwitcher>>,
    /// Auto-approve security-gated tool calls (scheduled automation mode).
    pub yolo_mode: bool,
    /// Checkpoint persistence policy.
    pub checkpoint_durability: CheckpointDurability,
    /// Optional callback to persist agent state checkpoints.
    pub checkpoint_callback: Option<CheckpointCallback>,
    /// Feature flags for conditional prompt section inclusion.
    pub prompt_flags: PromptFlags,
    /// Maximum number of tool calls that can execute concurrently (default: 100).
    pub max_tool_concurrency: usize,
}

impl AgentConfig {
    /// Create a new agent config with a goal
    pub fn new(goal: impl Into<String>) -> Self {
        Self {
            goal: goal.into(),
            system_prompt: None,
            max_iterations: 100,
            temperature: None, // None = use model default
            context: HashMap::new(),
            tool_timeout: Duration::from_secs(300),
            max_tool_result_length: 4000,
            context_window: 128_000,
            max_output_tokens: None,
            agent_context: None,
            inject_agent_context: true,
            resource_limits: ResourceLimits::default(),
            stuck_detection: Some(StuckDetectorConfig::default()),
            scratchpad: None,
            model_routing: None,
            model_switcher: None,
            yolo_mode: false,
            checkpoint_durability: CheckpointDurability::Periodic { interval: 5 },
            checkpoint_callback: None,
            prompt_flags: PromptFlags::default(),
            max_tool_concurrency: DEFAULT_MAX_TOOL_CONCURRENCY,
        }
    }

    /// Set context window size in tokens.
    pub fn with_context_window(mut self, context_window: usize) -> Self {
        self.context_window = context_window;
        self
    }

    /// Set max output tokens for each LLM request.
    pub fn with_max_output_tokens(mut self, max_output_tokens: u32) -> Self {
        self.max_output_tokens = Some(max_output_tokens);
        self
    }

    /// Set custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set max iterations
    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Add context variable
    pub fn with_context(mut self, key: impl Into<String>, value: Value) -> Self {
        self.context.insert(key.into(), value);
        self
    }

    /// Set tool timeout (wrapper timeout).
    ///
    /// This should be >= the tool-internal timeout (e.g., `bash_timeout_secs`)
    /// plus a small buffer to avoid confusing error messages.
    pub fn with_tool_timeout(mut self, timeout: Duration) -> Self {
        self.tool_timeout = timeout;
        self
    }

    /// Set max tool result length
    pub fn with_max_tool_result_length(mut self, max: usize) -> Self {
        self.max_tool_result_length = max;
        self
    }

    /// Set temperature
    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    /// Set agent context for prompt injection
    pub fn with_agent_context(mut self, context: AgentContext) -> Self {
        self.agent_context = Some(context);
        self
    }

    /// Set whether to inject agent_context into system prompt.
    pub fn with_inject_agent_context(mut self, inject: bool) -> Self {
        self.inject_agent_context = inject;
        self
    }

    /// Set resource limits for guardrails.
    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.resource_limits = limits;
        self
    }

    /// Set stuck detection configuration.
    pub fn with_stuck_detection(mut self, config: StuckDetectorConfig) -> Self {
        self.stuck_detection = Some(config);
        self
    }

    /// Disable stuck detection.
    pub fn without_stuck_detection(mut self) -> Self {
        self.stuck_detection = None;
        self
    }

    /// Set scratchpad for append-only JSONL execution tracing.
    pub fn with_scratchpad(mut self, scratchpad: Arc<Scratchpad>) -> Self {
        self.scratchpad = Some(scratchpad);
        self
    }

    /// Set model routing configuration.
    pub fn with_model_routing(mut self, routing: ModelRoutingConfig) -> Self {
        self.model_routing = Some(routing);
        self
    }

    /// Set model switcher used by routing.
    pub fn with_model_switcher(mut self, switcher: Arc<dyn ModelSwitcher>) -> Self {
        self.model_switcher = Some(switcher);
        self
    }

    /// Enable or disable yolo mode (auto-approval execution mode).
    /// Set prompt flags for conditional section inclusion.
    pub fn with_prompt_flags(mut self, flags: PromptFlags) -> Self {
        self.prompt_flags = flags;
        self
    }

    pub fn with_yolo_mode(mut self, yolo_mode: bool) -> Self {
        self.yolo_mode = yolo_mode;
        self
    }

    /// Set checkpoint durability policy.
    pub fn with_checkpoint_durability(mut self, durability: CheckpointDurability) -> Self {
        self.checkpoint_durability = durability;
        self
    }

    /// Set asynchronous checkpoint callback.
    pub fn with_checkpoint_callback<F, Fut>(mut self, callback: F) -> Self
    where
        F: Fn(&AgentState) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<()>> + Send + 'static,
    {
        self.checkpoint_callback = Some(Arc::new(move |state| Box::pin(callback(state))));
        self
    }
}

/// Result of agent execution
#[derive(Debug)]
pub struct AgentResult {
    pub success: bool,
    pub answer: Option<String>,
    pub error: Option<String>,
    pub iterations: usize,
    pub total_tokens: u32,
    pub total_cost_usd: f64,
    pub state: AgentState,
    /// Resource usage snapshot at end of run.
    pub resource_usage: ResourceUsage,
}
