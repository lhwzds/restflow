//! Approval manager for handling pending command approvals.
//!
//! This module manages the lifecycle of approval requests and provides
//! callbacks for notifying users about pending approvals.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::security::{ApprovalStatus, PendingApproval};

/// Callback trait for approval notifications.
///
/// Implement this trait to receive notifications when approval is needed
/// (e.g., send a Telegram message asking for approval).
#[async_trait]
pub trait ApprovalCallback: Send + Sync {
    /// Called when a new approval request is created.
    ///
    /// The implementation should notify the user and provide a way for them
    /// to approve or reject the command.
    async fn request_approval(&self, approval: &PendingApproval) -> anyhow::Result<()>;

    /// Called when an approval request is resolved (approved, rejected, or expired).
    async fn on_resolved(&self, approval: &PendingApproval) -> anyhow::Result<()> {
        // Default implementation does nothing
        let _ = approval;
        Ok(())
    }
}

/// Manager for pending approval requests.
///
/// Handles creating, storing, and resolving approval requests. Supports
/// callbacks for notifying users about pending approvals.
pub struct ApprovalManager {
    /// Map of approval ID to pending approval
    pending: RwLock<HashMap<String, PendingApproval>>,

    /// Optional callback for approval notifications
    callback: Option<Arc<dyn ApprovalCallback>>,

    /// Default timeout in seconds for new approvals
    default_timeout_secs: u64,
}

impl ApprovalManager {
    /// Create a new approval manager with default timeout.
    pub fn new() -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            callback: None,
            default_timeout_secs: 300,
        }
    }

    /// Create a new approval manager with a custom default timeout.
    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self {
            pending: RwLock::new(HashMap::new()),
            callback: None,
            default_timeout_secs: timeout_secs,
        }
    }

    /// Set the callback for approval notifications.
    pub fn with_callback(mut self, callback: Arc<dyn ApprovalCallback>) -> Self {
        self.callback = Some(callback);
        self
    }

    /// Set the callback for approval notifications (mutable).
    pub fn set_callback(&mut self, callback: Arc<dyn ApprovalCallback>) {
        self.callback = Some(callback);
    }

    /// Create a new approval request for a command.
    ///
    /// If an identical pending approval already exists for the same task and
    /// command, returns its ID instead of creating a duplicate.
    ///
    /// Returns the approval ID that can be used to check status or resolve the request.
    pub async fn create_approval(
        &self,
        command: impl Into<String>,
        task_id: impl Into<String>,
        agent_id: impl Into<String>,
        workdir: Option<String>,
    ) -> anyhow::Result<String> {
        let command = command.into();
        let task_id = task_id.into();

        // Check for existing pending approval with same task+command to avoid duplicates.
        {
            let pending = self.pending.read().await;
            for existing in pending.values() {
                if existing.task_id == task_id
                    && existing.command == command
                    && existing.status == ApprovalStatus::Pending
                {
                    return Ok(existing.id.clone());
                }
            }
        }

        let mut approval =
            PendingApproval::new(command, task_id, agent_id, self.default_timeout_secs);

        if let Some(wd) = workdir {
            approval = approval.with_workdir(wd);
        }

        let id = approval.id.clone();

        // Store the approval
        {
            let mut pending = self.pending.write().await;
            pending.insert(id.clone(), approval.clone());
        }

        // Notify via callback
        if let Some(callback) = &self.callback {
            callback.request_approval(&approval).await?;
        }

        Ok(id)
    }

    /// Get a pending approval by ID.
    pub async fn get(&self, id: &str) -> Option<PendingApproval> {
        let pending = self.pending.read().await;
        pending.get(id).cloned()
    }

    /// Get all pending approvals.
    pub async fn get_all_pending(&self) -> Vec<PendingApproval> {
        let pending = self.pending.read().await;
        pending
            .values()
            .filter(|a| a.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }

    /// Approve a pending request.
    pub async fn approve(&self, id: &str) -> anyhow::Result<Option<PendingApproval>> {
        let mut pending = self.pending.write().await;

        if let Some(approval) = pending.get_mut(id) {
            if approval.is_expired() {
                approval.expire();
                let resolved = approval.clone();
                if let Some(callback) = &self.callback {
                    drop(pending); // Release lock before async call
                    callback.on_resolved(&resolved).await?;
                }
                return Ok(Some(resolved));
            }

            if approval.status != ApprovalStatus::Pending {
                return Ok(Some(approval.clone()));
            }

            approval.approve();
            let resolved = approval.clone();

            if let Some(callback) = &self.callback {
                drop(pending); // Release lock before async call
                callback.on_resolved(&resolved).await?;
            }

            Ok(Some(resolved))
        } else {
            Ok(None)
        }
    }

    /// Reject a pending request.
    pub async fn reject(
        &self,
        id: &str,
        reason: Option<String>,
    ) -> anyhow::Result<Option<PendingApproval>> {
        let mut pending = self.pending.write().await;

        if let Some(approval) = pending.get_mut(id) {
            if approval.status != ApprovalStatus::Pending {
                return Ok(Some(approval.clone()));
            }

            approval.reject(reason);
            let resolved = approval.clone();

            if let Some(callback) = &self.callback {
                drop(pending); // Release lock before async call
                callback.on_resolved(&resolved).await?;
            }

            Ok(Some(resolved))
        } else {
            Ok(None)
        }
    }

    /// Check the status of an approval request.
    ///
    /// This also handles expiration - if the request has expired, it will be
    /// marked as expired.
    pub async fn check_status(&self, id: &str) -> Option<ApprovalStatus> {
        let mut pending = self.pending.write().await;

        if let Some(approval) = pending.get_mut(id) {
            if approval.status == ApprovalStatus::Pending && approval.is_expired() {
                approval.expire();
                if let Some(callback) = &self.callback {
                    let resolved = approval.clone();
                    drop(pending); // Release lock before async call
                    let _ = callback.on_resolved(&resolved).await;
                }
                return Some(ApprovalStatus::Expired);
            }
            Some(approval.status)
        } else {
            None
        }
    }

    /// Remove a resolved approval from the manager.
    pub async fn remove(&self, id: &str) -> Option<PendingApproval> {
        let mut pending = self.pending.write().await;
        pending.remove(id)
    }

    /// Clean up expired approvals.
    ///
    /// Returns the number of approvals that were expired.
    pub async fn cleanup_expired(&self) -> usize {
        let mut pending = self.pending.write().await;
        let mut expired_approvals = Vec::new();

        for approval in pending.values_mut() {
            if approval.status == ApprovalStatus::Pending && approval.is_expired() {
                approval.expire();
                expired_approvals.push(approval.clone());
            }
        }
        drop(pending);

        // Notify callbacks for expired approvals
        if let Some(callback) = &self.callback {
            for approval in &expired_approvals {
                let _ = callback.on_resolved(approval).await;
            }
        }

        expired_approvals.len()
    }

    /// Get pending approvals for a specific task.
    pub async fn get_for_task(&self, task_id: &str) -> Vec<PendingApproval> {
        let pending = self.pending.read().await;
        pending
            .values()
            .filter(|a| a.task_id == task_id && a.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }

    /// Get pending approvals for a specific agent.
    pub async fn get_for_agent(&self, agent_id: &str) -> Vec<PendingApproval> {
        let pending = self.pending.read().await;
        pending
            .values()
            .filter(|a| a.agent_id == agent_id && a.status == ApprovalStatus::Pending)
            .cloned()
            .collect()
    }
}

impl Default for ApprovalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_approval_manager_new() {
        let manager = ApprovalManager::new();
        assert!(manager.get_all_pending().await.is_empty());
    }

    #[tokio::test]
    async fn test_create_approval() {
        let manager = ApprovalManager::new();
        let id = manager
            .create_approval("rm -rf temp", "task-1", "agent-1", None)
            .await
            .unwrap();

        let approval = manager.get(&id).await.unwrap();
        assert_eq!(approval.command, "rm -rf temp");
        assert_eq!(approval.task_id, "task-1");
        assert_eq!(approval.agent_id, "agent-1");
        assert_eq!(approval.status, ApprovalStatus::Pending);
    }

    #[tokio::test]
    async fn test_create_approval_with_workdir() {
        let manager = ApprovalManager::new();
        let id = manager
            .create_approval("ls", "task-1", "agent-1", Some("/tmp".to_string()))
            .await
            .unwrap();

        let approval = manager.get(&id).await.unwrap();
        assert_eq!(approval.workdir, Some("/tmp".to_string()));
    }

    #[tokio::test]
    async fn test_approve_request() {
        let manager = ApprovalManager::new();
        let id = manager
            .create_approval("rm file", "task-1", "agent-1", None)
            .await
            .unwrap();

        let result = manager.approve(&id).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().status, ApprovalStatus::Approved);

        // Check the stored approval is also updated
        let approval = manager.get(&id).await.unwrap();
        assert_eq!(approval.status, ApprovalStatus::Approved);
    }

    #[tokio::test]
    async fn test_reject_request() {
        let manager = ApprovalManager::new();
        let id = manager
            .create_approval("rm file", "task-1", "agent-1", None)
            .await
            .unwrap();

        let result = manager
            .reject(&id, Some("Too dangerous".to_string()))
            .await
            .unwrap();
        assert!(result.is_some());
        let approval = result.unwrap();
        assert_eq!(approval.status, ApprovalStatus::Rejected);
        assert_eq!(approval.rejection_reason, Some("Too dangerous".to_string()));
    }

    #[tokio::test]
    async fn test_approve_nonexistent() {
        let manager = ApprovalManager::new();
        let result = manager.approve("nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_reject_nonexistent() {
        let manager = ApprovalManager::new();
        let result = manager.reject("nonexistent", None).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_all_pending() {
        let manager = ApprovalManager::new();

        manager
            .create_approval("cmd1", "task-1", "agent-1", None)
            .await
            .unwrap();
        let id2 = manager
            .create_approval("cmd2", "task-1", "agent-1", None)
            .await
            .unwrap();
        manager
            .create_approval("cmd3", "task-1", "agent-1", None)
            .await
            .unwrap();

        // Approve one
        manager.approve(&id2).await.unwrap();

        let pending = manager.get_all_pending().await;
        assert_eq!(pending.len(), 2);
    }

    #[tokio::test]
    async fn test_get_for_task() {
        let manager = ApprovalManager::new();

        manager
            .create_approval("cmd1", "task-1", "agent-1", None)
            .await
            .unwrap();
        manager
            .create_approval("cmd2", "task-2", "agent-1", None)
            .await
            .unwrap();
        manager
            .create_approval("cmd3", "task-1", "agent-1", None)
            .await
            .unwrap();

        let task1_approvals = manager.get_for_task("task-1").await;
        assert_eq!(task1_approvals.len(), 2);

        let task2_approvals = manager.get_for_task("task-2").await;
        assert_eq!(task2_approvals.len(), 1);
    }

    #[tokio::test]
    async fn test_get_for_agent() {
        let manager = ApprovalManager::new();

        manager
            .create_approval("cmd1", "task-1", "agent-1", None)
            .await
            .unwrap();
        manager
            .create_approval("cmd2", "task-2", "agent-2", None)
            .await
            .unwrap();
        manager
            .create_approval("cmd3", "task-3", "agent-1", None)
            .await
            .unwrap();

        let agent1_approvals = manager.get_for_agent("agent-1").await;
        assert_eq!(agent1_approvals.len(), 2);

        let agent2_approvals = manager.get_for_agent("agent-2").await;
        assert_eq!(agent2_approvals.len(), 1);
    }

    #[tokio::test]
    async fn test_remove_approval() {
        let manager = ApprovalManager::new();
        let id = manager
            .create_approval("cmd", "task-1", "agent-1", None)
            .await
            .unwrap();

        let removed = manager.remove(&id).await;
        assert!(removed.is_some());

        let get_result = manager.get(&id).await;
        assert!(get_result.is_none());
    }

    #[tokio::test]
    async fn test_check_status() {
        let manager = ApprovalManager::new();
        let id = manager
            .create_approval("cmd", "task-1", "agent-1", None)
            .await
            .unwrap();

        let status = manager.check_status(&id).await;
        assert_eq!(status, Some(ApprovalStatus::Pending));

        manager.approve(&id).await.unwrap();
        let status = manager.check_status(&id).await;
        assert_eq!(status, Some(ApprovalStatus::Approved));
    }

    #[tokio::test]
    async fn test_check_status_nonexistent() {
        let manager = ApprovalManager::new();
        let status = manager.check_status("nonexistent").await;
        assert!(status.is_none());
    }

    #[tokio::test]
    async fn test_with_timeout() {
        let manager = ApprovalManager::with_timeout(60);
        let id = manager
            .create_approval("cmd", "task-1", "agent-1", None)
            .await
            .unwrap();

        let approval = manager.get(&id).await.unwrap();
        // The timeout should be 60 seconds
        assert_eq!(approval.expires_at - approval.created_at, 60);
    }

    #[tokio::test]
    async fn test_double_approve() {
        let manager = ApprovalManager::new();
        let id = manager
            .create_approval("cmd", "task-1", "agent-1", None)
            .await
            .unwrap();

        // First approve
        manager.approve(&id).await.unwrap();

        // Second approve should return the already-approved status
        let result = manager.approve(&id).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().status, ApprovalStatus::Approved);
    }

    #[tokio::test]
    async fn test_double_reject() {
        let manager = ApprovalManager::new();
        let id = manager
            .create_approval("cmd", "task-1", "agent-1", None)
            .await
            .unwrap();

        // First reject
        manager.reject(&id, None).await.unwrap();

        // Second reject should return the already-rejected status
        let result = manager.reject(&id, None).await.unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().status, ApprovalStatus::Rejected);
    }

    // Test with mock callback
    struct MockCallback {
        request_count: RwLock<usize>,
        resolve_count: RwLock<usize>,
    }

    impl MockCallback {
        fn new() -> Self {
            Self {
                request_count: RwLock::new(0),
                resolve_count: RwLock::new(0),
            }
        }
    }

    #[async_trait]
    impl ApprovalCallback for MockCallback {
        async fn request_approval(&self, _approval: &PendingApproval) -> anyhow::Result<()> {
            let mut count = self.request_count.write().await;
            *count += 1;
            Ok(())
        }

        async fn on_resolved(&self, _approval: &PendingApproval) -> anyhow::Result<()> {
            let mut count = self.resolve_count.write().await;
            *count += 1;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_callback_on_create() {
        let callback = Arc::new(MockCallback::new());
        let manager = ApprovalManager::new().with_callback(callback.clone());

        manager
            .create_approval("cmd", "task-1", "agent-1", None)
            .await
            .unwrap();

        let count = *callback.request_count.read().await;
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_callback_on_approve() {
        let callback = Arc::new(MockCallback::new());
        let manager = ApprovalManager::new().with_callback(callback.clone());

        let id = manager
            .create_approval("cmd", "task-1", "agent-1", None)
            .await
            .unwrap();
        manager.approve(&id).await.unwrap();

        let resolve_count = *callback.resolve_count.read().await;
        assert_eq!(resolve_count, 1);
    }

    #[tokio::test]
    async fn test_callback_on_reject() {
        let callback = Arc::new(MockCallback::new());
        let manager = ApprovalManager::new().with_callback(callback.clone());

        let id = manager
            .create_approval("cmd", "task-1", "agent-1", None)
            .await
            .unwrap();
        manager.reject(&id, None).await.unwrap();

        let resolve_count = *callback.resolve_count.read().await;
        assert_eq!(resolve_count, 1);
    }
}
