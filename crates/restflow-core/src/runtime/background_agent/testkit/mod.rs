//! Test utilities for deterministic background-agent stress tests.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Result, anyhow};
use tokio::sync::{RwLock, mpsc};
use tokio::time::{Duration, sleep};

use crate::models::{BackgroundAgent, MemoryConfig, NotificationConfig, SteerMessage};
use crate::runtime::background_agent::runner::{
    AgentExecutor, ExecutionResult, NotificationSender,
};
use crate::storage::BackgroundAgentStorage;

/// Creates a temporary storage instance for tests.
pub fn create_test_storage() -> (Arc<BackgroundAgentStorage>, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let db_path = temp_dir.path().join("stress-test.db");
    let db = Arc::new(redb::Database::create(db_path).expect("failed to create redb database"));
    (
        Arc::new(BackgroundAgentStorage::new(db).expect("failed to init task storage")),
        temp_dir,
    )
}

/// Deterministic mock executor for stress scenarios.
///
/// It can inject a fixed delay and fail every Nth call.
pub struct DeterministicMockExecutor {
    call_count: AtomicU32,
    delay_ms: u64,
    fail_every: Option<u32>,
}

impl DeterministicMockExecutor {
    pub fn new(delay_ms: u64, fail_every: Option<u32>) -> Self {
        Self {
            call_count: AtomicU32::new(0),
            delay_ms,
            fail_every,
        }
    }

    pub fn call_count(&self) -> u32 {
        self.call_count.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl AgentExecutor for DeterministicMockExecutor {
    async fn execute(
        &self,
        agent_id: &str,
        background_task_id: Option<&str>,
        _input: Option<&str>,
        _memory_config: &MemoryConfig,
        _steer_rx: Option<mpsc::Receiver<SteerMessage>>,
    ) -> Result<ExecutionResult> {
        let call_index = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;

        if self.delay_ms > 0 {
            sleep(Duration::from_millis(self.delay_ms)).await;
        }

        if self
            .fail_every
            .is_some_and(|interval| interval > 0 && call_index % interval == 0)
        {
            return Err(anyhow!(
                "deterministic mock failure at call {} for task {:?}",
                call_index,
                background_task_id
            ));
        }

        Ok(ExecutionResult::success(
            format!("mock-executed: agent={agent_id}, call={call_index}"),
            Vec::new(),
        ))
    }
}

/// In-memory notification sink for assertions.
#[derive(Default)]
pub struct MockNotificationSender {
    notifications: Arc<RwLock<Vec<(String, bool, String)>>>,
}

impl MockNotificationSender {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn notification_count(&self) -> usize {
        self.notifications.read().await.len()
    }
}

#[async_trait::async_trait]
impl NotificationSender for MockNotificationSender {
    async fn send(
        &self,
        _config: &NotificationConfig,
        task: &BackgroundAgent,
        success: bool,
        message: &str,
    ) -> Result<()> {
        self.notifications
            .write()
            .await
            .push((task.id.clone(), success, message.to_string()));
        Ok(())
    }

    async fn send_formatted(&self, message: &str) -> Result<()> {
        self.notifications
            .write()
            .await
            .push(("formatted".to_string(), true, message.to_string()));
        Ok(())
    }
}
