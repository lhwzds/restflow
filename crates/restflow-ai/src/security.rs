use async_trait::async_trait;

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct SecurityDecision {
    pub allowed: bool,
    pub requires_approval: bool,
    pub approval_id: Option<String>,
    pub reason: Option<String>,
}

impl SecurityDecision {
    pub fn allowed(reason: Option<String>) -> Self {
        Self {
            allowed: true,
            requires_approval: false,
            approval_id: None,
            reason,
        }
    }

    pub fn blocked(reason: Option<String>) -> Self {
        Self {
            allowed: false,
            requires_approval: false,
            approval_id: None,
            reason,
        }
    }

    pub fn requires_approval(approval_id: String, reason: Option<String>) -> Self {
        Self {
            allowed: false,
            requires_approval: true,
            approval_id: Some(approval_id),
            reason,
        }
    }
}

#[async_trait]
pub trait SecurityGate: Send + Sync {
    async fn check_command(
        &self,
        command: &str,
        task_id: &str,
        agent_id: &str,
        workdir: Option<&str>,
    ) -> Result<SecurityDecision>;
}
