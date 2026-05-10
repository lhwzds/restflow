//! Composable toolset abstraction.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::error::{Result, ToolError};
use crate::tool::{Tool, ToolOutput, ToolSchema};

/// Intercepts tool execution and optionally delegates to the next tool in the chain.
#[async_trait]
pub trait ToolWrapper: Send + Sync {
    fn wrapper_name(&self) -> &str;

    async fn wrap_execute(
        &self,
        tool_name: &str,
        input: Value,
        next: &dyn Tool,
    ) -> Result<ToolOutput>;
}

/// A tool implementation that applies wrappers around an inner tool.
pub struct WrappedTool {
    inner: Arc<dyn Tool>,
    wrappers: Vec<Arc<dyn ToolWrapper>>,
}

impl WrappedTool {
    pub fn new(inner: Arc<dyn Tool>, wrappers: Vec<Arc<dyn ToolWrapper>>) -> Self {
        Self { inner, wrappers }
    }

    pub fn inner(&self) -> &Arc<dyn Tool> {
        &self.inner
    }
}

struct RemainingChain<'a> {
    tool_name: &'a str,
    inner: &'a dyn Tool,
    wrappers: &'a [Arc<dyn ToolWrapper>],
    index: usize,
}

#[async_trait]
impl Tool for RemainingChain<'_> {
    fn name(&self) -> &str {
        self.tool_name
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> Value {
        self.inner.parameters_schema()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        execute_chain(self.tool_name, self.inner, self.wrappers, self.index, input).await
    }
}

async fn execute_chain(
    tool_name: &str,
    inner: &dyn Tool,
    wrappers: &[Arc<dyn ToolWrapper>],
    index: usize,
    input: Value,
) -> Result<ToolOutput> {
    if index >= wrappers.len() {
        return inner.execute(input).await;
    }

    let next = RemainingChain {
        tool_name,
        inner,
        wrappers,
        index: index + 1,
    };
    wrappers[index].wrap_execute(tool_name, input, &next).await
}

#[async_trait]
impl Tool for WrappedTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn parameters_schema(&self) -> Value {
        self.inner.parameters_schema()
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput> {
        execute_chain(self.name(), self.inner.as_ref(), &self.wrappers, 0, input).await
    }
}

/// Wrapper that enforces a timeout per tool call.
pub struct TimeoutWrapper {
    timeout: Duration,
}

impl TimeoutWrapper {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout }
    }
}

#[async_trait]
impl ToolWrapper for TimeoutWrapper {
    fn wrapper_name(&self) -> &str {
        "timeout"
    }

    async fn wrap_execute(
        &self,
        tool_name: &str,
        input: Value,
        next: &dyn Tool,
    ) -> Result<ToolOutput> {
        match tokio::time::timeout(self.timeout, next.execute(input)).await {
            Ok(result) => result,
            Err(_) => Err(ToolError::Tool(format!(
                "Tool '{tool_name}' timed out after {}ms",
                self.timeout.as_millis()
            ))),
        }
    }
}

/// Wrapper that limits concurrent executions of the wrapped tool.
pub struct RateLimitWrapper {
    semaphore: Arc<Semaphore>,
}

impl RateLimitWrapper {
    pub fn new(max_concurrent: usize) -> Self {
        let permits = max_concurrent.max(1);
        Self {
            semaphore: Arc::new(Semaphore::new(permits)),
        }
    }
}

#[async_trait]
impl ToolWrapper for RateLimitWrapper {
    fn wrapper_name(&self) -> &str {
        "rate_limit"
    }

    async fn wrap_execute(
        &self,
        tool_name: &str,
        input: Value,
        next: &dyn Tool,
    ) -> Result<ToolOutput> {
        let _permit = self.semaphore.acquire().await.map_err(|_| {
            ToolError::Tool(format!(
                "Rate limiter for tool '{tool_name}' is unavailable"
            ))
        })?;
        next.execute(input).await
    }
}

/// Registry for managing available tools.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        let name = tool.name().to_string();
        self.tools.insert(name, Arc::new(tool));
    }

    pub fn register_arc(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.name().to_string();
        self.tools.insert(name, tool);
    }

    pub fn register_wrapped_arc(
        &mut self,
        tool: Arc<dyn Tool>,
        wrappers: Vec<Arc<dyn ToolWrapper>>,
    ) {
        let wrapped = Arc::new(WrappedTool::new(tool, wrappers));
        let name = wrapped.name().to_string();
        self.tools.insert(name, wrapped);
    }

    pub fn register_wrapped<T: Tool + 'static>(
        &mut self,
        tool: T,
        wrappers: Vec<Arc<dyn ToolWrapper>>,
    ) {
        self.register_wrapped_arc(Arc::new(tool), wrappers);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn has(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    pub fn list(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }

    pub fn schemas(&self) -> Vec<ToolSchema> {
        self.tools.values().map(|t| t.schema()).collect()
    }

    pub async fn execute(&self, name: &str, input: Value) -> Result<ToolOutput> {
        let tool = self
            .get(name)
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;
        tool.execute(input).await
    }

    pub async fn execute_safe(&self, name: &str, input: Value) -> Result<ToolOutput> {
        self.execute(name, input).await
    }
}

pub type ToolPredicate = Arc<dyn Fn(&ToolSchema) -> bool + Send + Sync>;

/// Toolset wrapper that filters visible/callable tools by predicate.
pub struct FilteredToolset<T> {
    inner: T,
    predicate: ToolPredicate,
}

impl<T> FilteredToolset<T> {
    pub fn new(inner: T, predicate: ToolPredicate) -> Self {
        Self { inner, predicate }
    }
}

impl<T: Toolset> FilteredToolset<T> {
    pub fn from_allowlist(inner: T, allowed_tools: &[String]) -> Self {
        let allowed: HashSet<String> = allowed_tools
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        let predicate = Arc::new(move |tool: &ToolSchema| {
            if allowed.is_empty() {
                return true;
            }
            allowed.contains(&tool.name)
        });

        Self::new(inner, predicate)
    }
}

#[async_trait]
impl<T: Toolset> Toolset for FilteredToolset<T> {
    fn list_tools(&self) -> Vec<ToolSchema> {
        self.inner
            .list_tools()
            .into_iter()
            .filter(|tool| (self.predicate)(tool))
            .collect()
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput> {
        if !self
            .list_tools()
            .iter()
            .any(|tool| tool.name.as_str() == name)
        {
            return Err(ToolError::NotFound(name.to_string()));
        }
        self.inner.call_tool(name, args).await
    }

    async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput> {
        if !self
            .list_tools()
            .iter()
            .any(|tool| tool.name.as_str() == name)
        {
            return Err(ToolError::NotFound(name.to_string()));
        }
        self.inner.call_tool_safe(name, args).await
    }
}

/// Runtime context for optional per-step toolset preparation.
#[derive(Debug, Clone, Default)]
pub struct ToolsetContext {
    pub step: Option<usize>,
    pub agent_id: Option<String>,
}

/// Common abstraction over different toolset implementations.
#[async_trait]
pub trait Toolset: Send + Sync {
    /// List schemas for all currently available tools.
    fn list_tools(&self) -> Vec<ToolSchema>;

    /// Call a tool by name.
    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput>;

    /// Call a tool with parallel-safety semantics.
    async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput>;

    /// Optional preparation callback before each step.
    async fn prepare(&self, _context: &ToolsetContext) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl Toolset for ToolRegistry {
    fn list_tools(&self) -> Vec<ToolSchema> {
        self.schemas()
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput> {
        self.execute(name, args).await
    }

    async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput> {
        self.execute_safe(name, args).await
    }
}

#[async_trait]
impl<T: Toolset + ?Sized> Toolset for Arc<T> {
    fn list_tools(&self) -> Vec<ToolSchema> {
        self.as_ref().list_tools()
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<ToolOutput> {
        self.as_ref().call_tool(name, args).await
    }

    async fn call_tool_safe(&self, name: &str, args: Value) -> Result<ToolOutput> {
        self.as_ref().call_tool_safe(name, args).await
    }

    async fn prepare(&self, context: &ToolsetContext) -> Result<()> {
        self.as_ref().prepare(context).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use serde_json::json;
    use tokio::time::sleep;

    use super::*;

    struct EchoTool;
    struct ReverseTool;

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str {
            "echo"
        }

        fn description(&self) -> &str {
            "Echo input"
        }

        fn parameters_schema(&self) -> Value {
            json!({"type":"object"})
        }

        async fn execute(&self, input: Value) -> Result<ToolOutput> {
            Ok(ToolOutput::success(input))
        }
    }

    #[async_trait]
    impl Tool for ReverseTool {
        fn name(&self) -> &str {
            "reverse"
        }

        fn description(&self) -> &str {
            "Reverse input"
        }

        fn parameters_schema(&self) -> Value {
            json!({"type":"object"})
        }

        async fn execute(&self, input: Value) -> Result<ToolOutput> {
            Ok(ToolOutput::success(input))
        }
    }

    #[test]
    fn registry_as_toolset_lists_tools() {
        let registry = ToolRegistry::new();
        let tools = Toolset::list_tools(&registry);
        assert!(tools.is_empty());
    }

    #[tokio::test]
    async fn registry_as_toolset_call_unknown_fails() {
        let registry = ToolRegistry::new();
        let result = Toolset::call_tool(&registry, "missing_tool", json!({})).await;
        assert!(result.is_err());
    }

    #[test]
    fn allowlist_filters_tool_schemas() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);
        registry.register(ReverseTool);

        let toolset = FilteredToolset::from_allowlist(registry, &["echo".to_string()]);
        let names: Vec<String> = toolset
            .list_tools()
            .into_iter()
            .map(|schema| schema.name)
            .collect();

        assert_eq!(names, vec!["echo".to_string()]);
    }

    #[tokio::test]
    async fn blocked_tool_call_returns_not_found() {
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool);
        registry.register(ReverseTool);

        let toolset = FilteredToolset::from_allowlist(registry, &["echo".to_string()]);
        let err = toolset
            .call_tool("reverse", json!({"text":"hello"}))
            .await
            .unwrap_err();

        assert!(matches!(err, ToolError::NotFound(ref name) if name == "reverse"));
    }

    struct TraceWrapper {
        name: &'static str,
        trace: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl ToolWrapper for TraceWrapper {
        fn wrapper_name(&self) -> &str {
            self.name
        }

        async fn wrap_execute(
            &self,
            _tool_name: &str,
            input: Value,
            next: &dyn Tool,
        ) -> Result<ToolOutput> {
            self.trace
                .lock()
                .expect("trace mutex should not be poisoned")
                .push(format!("before:{}", self.name));
            let result = next.execute(input).await;
            self.trace
                .lock()
                .expect("trace mutex should not be poisoned")
                .push(format!("after:{}", self.name));
            result
        }
    }

    #[tokio::test]
    async fn wrapper_chain_executes_in_order() {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let wrappers: Vec<Arc<dyn ToolWrapper>> = vec![
            Arc::new(TraceWrapper {
                name: "w1",
                trace: trace.clone(),
            }),
            Arc::new(TraceWrapper {
                name: "w2",
                trace: trace.clone(),
            }),
        ];
        let tool = WrappedTool::new(Arc::new(EchoTool), wrappers);

        let output = tool
            .execute(json!({"msg":"hello"}))
            .await
            .expect("wrapped execution should succeed");
        assert!(output.success);
        let events = trace
            .lock()
            .expect("trace mutex should not be poisoned")
            .clone();
        assert_eq!(
            events,
            vec!["before:w1", "before:w2", "after:w2", "after:w1"]
        );
    }

    #[tokio::test]
    async fn timeout_wrapper_cancels_slow_tool() {
        struct SlowTool;

        #[async_trait]
        impl Tool for SlowTool {
            fn name(&self) -> &str {
                "slow"
            }

            fn description(&self) -> &str {
                "Slow tool"
            }

            fn parameters_schema(&self) -> Value {
                json!({"type":"object"})
            }

            async fn execute(&self, _input: Value) -> Result<ToolOutput> {
                sleep(Duration::from_millis(80)).await;
                Ok(ToolOutput::success(json!({"ok":true})))
            }
        }

        let wrapped = WrappedTool::new(
            Arc::new(SlowTool),
            vec![Arc::new(TimeoutWrapper::new(Duration::from_millis(20)))],
        );
        let error = wrapped
            .execute(json!({}))
            .await
            .expect_err("slow tool should timeout");
        assert!(error.to_string().contains("timed out"));
    }

    #[tokio::test]
    async fn rate_limit_wrapper_limits_concurrency() {
        struct CountingTool {
            in_flight: Arc<AtomicUsize>,
            max_seen: Arc<AtomicUsize>,
        }

        #[async_trait]
        impl Tool for CountingTool {
            fn name(&self) -> &str {
                "counting"
            }

            fn description(&self) -> &str {
                "Counting tool"
            }

            fn parameters_schema(&self) -> Value {
                json!({"type":"object"})
            }

            async fn execute(&self, _input: Value) -> Result<ToolOutput> {
                let current = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                self.max_seen.fetch_max(current, Ordering::SeqCst);
                sleep(Duration::from_millis(40)).await;
                self.in_flight.fetch_sub(1, Ordering::SeqCst);
                Ok(ToolOutput::success(json!({"ok":true})))
            }
        }

        let in_flight = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let wrapped = Arc::new(WrappedTool::new(
            Arc::new(CountingTool {
                in_flight: in_flight.clone(),
                max_seen: max_seen.clone(),
            }),
            vec![Arc::new(RateLimitWrapper::new(1))],
        ));

        let mut tasks = Vec::new();
        for _ in 0..3 {
            let tool = wrapped.clone();
            tasks.push(tokio::spawn(async move { tool.execute(json!({})).await }));
        }
        for task in tasks {
            let result = task.await.expect("task should join");
            assert!(result.is_ok());
        }

        assert_eq!(max_seen.load(Ordering::SeqCst), 1);
    }
}
