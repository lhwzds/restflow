use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeferredStatus {
    Pending,
    Approved,
    Denied { reason: String },
    TimedOut,
}

#[derive(Debug, Clone)]
pub struct DeferredToolCall {
    pub call_id: String,
    pub tool_name: String,
    pub args: Value,
    pub approval_id: Option<String>,
    pub status: DeferredStatus,
    pub created_at: Instant,
}

#[derive(Debug, Clone)]
pub struct DeferredResolution {
    pub call_id: String,
    pub tool_name: String,
    pub status: DeferredStatus,
}

pub struct DeferredExecutionManager {
    pending: RwLock<HashMap<String, DeferredToolCall>>,
    approval_index: RwLock<HashMap<String, String>>,
    timeout: Duration,
}

impl DeferredExecutionManager {
    pub fn new(timeout: Duration) -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            approval_index: RwLock::new(HashMap::new()),
            timeout,
        }
    }

    pub async fn defer(
        &self,
        call_id: &str,
        tool_name: &str,
        args: Value,
        approval_id: Option<String>,
    ) -> String {
        let deferred = DeferredToolCall {
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            args,
            approval_id: approval_id.clone(),
            status: DeferredStatus::Pending,
            created_at: Instant::now(),
        };

        let key = deferred.call_id.clone();
        self.pending.write().await.insert(key.clone(), deferred);
        if let Some(approval_id) = approval_id {
            self.approval_index
                .write()
                .await
                .insert(approval_id, key.clone());
        }
        key
    }

    pub async fn resolve(&self, call_id: &str, approved: bool, reason: Option<String>) -> bool {
        let mut pending = self.pending.write().await;
        let Some(call) = pending.get_mut(call_id) else {
            return false;
        };

        call.status = if approved {
            DeferredStatus::Approved
        } else {
            DeferredStatus::Denied {
                reason: reason.unwrap_or_else(|| "Denied by user".to_string()),
            }
        };
        true
    }

    pub async fn resolve_by_approval_id(
        &self,
        approval_id: &str,
        approved: bool,
        reason: Option<String>,
    ) -> bool {
        let call_id = self.approval_index.read().await.get(approval_id).cloned();
        let Some(call_id) = call_id else {
            return false;
        };
        self.resolve(&call_id, approved, reason).await
    }

    pub async fn drain_resolved(&self) -> Vec<DeferredToolCall> {
        let mut pending = self.pending.write().await;
        let mut approval_index = self.approval_index.write().await;
        let mut ready = Vec::new();
        let mut remove_ids = Vec::new();

        for (call_id, call) in pending.iter_mut() {
            if call.status == DeferredStatus::Pending && call.created_at.elapsed() >= self.timeout {
                call.status = DeferredStatus::TimedOut;
            }
            if call.status != DeferredStatus::Pending {
                remove_ids.push(call_id.clone());
            }
        }

        for call_id in remove_ids {
            if let Some(call) = pending.remove(&call_id) {
                if let Some(approval_id) = &call.approval_id {
                    approval_index.remove(approval_id);
                }
                ready.push(call);
            }
        }

        ready
    }

    pub async fn has_pending(&self) -> bool {
        !self.pending.read().await.is_empty()
    }

    pub async fn get_status(&self, call_id: &str) -> Option<DeferredStatus> {
        self.pending
            .read()
            .await
            .get(call_id)
            .map(|call| call.status.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_defer_and_resolve_by_call_id() {
        let manager = DeferredExecutionManager::new(Duration::from_secs(30));
        manager
            .defer("call-1", "bash", json!({"command":"echo hi"}), None)
            .await;
        assert!(manager.has_pending().await);

        let ok = manager.resolve("call-1", true, None).await;
        assert!(ok);
        let drained = manager.drain_resolved().await;
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].status, DeferredStatus::Approved);
        assert!(!manager.has_pending().await);
    }

    #[tokio::test]
    async fn test_resolve_by_approval_id() {
        let manager = DeferredExecutionManager::new(Duration::from_secs(30));
        manager
            .defer(
                "call-2",
                "bash",
                json!({"command":"rm -rf tmp"}),
                Some("approval-1".to_string()),
            )
            .await;

        assert!(
            manager
                .resolve_by_approval_id("approval-1", false, Some("No".to_string()))
                .await
        );
        let drained = manager.drain_resolved().await;
        assert_eq!(drained.len(), 1);
        assert_eq!(
            drained[0].status,
            DeferredStatus::Denied {
                reason: "No".to_string()
            }
        );
    }

    #[tokio::test]
    async fn test_timeout_promotes_to_timed_out() {
        let manager = DeferredExecutionManager::new(Duration::from_millis(20));
        manager
            .defer("call-3", "bash", json!({"command":"echo timeout"}), None)
            .await;
        tokio::time::sleep(Duration::from_millis(40)).await;

        let drained = manager.drain_resolved().await;
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].status, DeferredStatus::TimedOut);
    }
}
