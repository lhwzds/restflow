//! Composable tool wrappers (decorators) for policy enforcement.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Semaphore;

use crate::error::{Result, ToolError};
use crate::tool::{Tool, ToolOutput};

#[async_trait]
pub trait ToolWrapper: Send + Sync {
    /// Unique identifier for this wrapper type.
    fn wrapper_name(&self) -> &str;

    /// Intercept tool execution and optionally delegate to `next`.
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

    fn supports_parallel(&self) -> bool {
        self.inner.supports_parallel()
    }

    fn supports_parallel_for(&self, input: &Value) -> bool {
        self.inner.supports_parallel_for(input)
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

    fn supports_parallel(&self) -> bool {
        self.inner.supports_parallel()
    }

    fn supports_parallel_for(&self, input: &Value) -> bool {
        self.inner.supports_parallel_for(input)
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

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use serde_json::json;
    use tokio::time::sleep;

    use super::*;

    struct EchoTool;

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

    struct SlowTool {
        delay: Duration,
    }

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
            sleep(self.delay).await;
            Ok(ToolOutput::success(json!({"ok":true})))
        }
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
        let wrapped = WrappedTool::new(
            Arc::new(SlowTool {
                delay: Duration::from_millis(80),
            }),
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
