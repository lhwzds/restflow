use crate::hooks::{BackgroundAgentHookScheduler, HookExecutor};
use crate::models::{Hook, HookContext, HookEvent};
use crate::storage::{BackgroundAgentStorage, HookStorage, Storage};
use anyhow::{Result, anyhow};
use std::sync::Arc;

#[derive(Clone)]
pub struct HookCapabilityService {
    hooks: HookStorage,
    background_agents: BackgroundAgentStorage,
}

impl HookCapabilityService {
    pub fn new(hooks: HookStorage, background_agents: BackgroundAgentStorage) -> Self {
        Self {
            hooks,
            background_agents,
        }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        Self::new(storage.hooks.clone(), storage.background_agents.clone())
    }

    pub fn list(&self) -> Result<Vec<Hook>> {
        self.hooks.list()
    }

    pub fn create(&self, hook: Hook) -> Result<Hook> {
        self.hooks.create(&hook)?;
        Ok(hook)
    }

    pub fn update(&self, id: &str, hook: Hook) -> Result<Hook> {
        self.hooks.update(id, &hook)?;
        Ok(hook)
    }

    pub fn delete(&self, id: &str) -> Result<bool> {
        self.hooks.delete(id)
    }

    pub async fn test(&self, id: &str) -> Result<()> {
        let hook = self
            .hooks
            .get(id)?
            .ok_or_else(|| anyhow!("Hook not found: {id}"))?;
        let scheduler = Arc::new(BackgroundAgentHookScheduler::new(
            self.background_agents.clone(),
        ));
        let executor =
            HookExecutor::with_storage(self.hooks.clone()).with_task_scheduler(scheduler);
        executor
            .execute_hook(&hook, &sample_hook_context(&hook.event))
            .await
    }
}

fn sample_hook_context(event: &HookEvent) -> HookContext {
    let timestamp = chrono::Utc::now().timestamp_millis();
    let mut context = HookContext {
        event: event.clone(),
        task_id: "hook-test-task".to_string(),
        task_name: "Hook Test Task".to_string(),
        agent_id: "hook-test-agent".to_string(),
        success: None,
        output: None,
        error: None,
        duration_ms: None,
        timestamp,
    };

    match event {
        HookEvent::TaskStarted => {}
        HookEvent::TaskCompleted => {
            context.success = Some(true);
            context.output = Some("Sample hook output".to_string());
            context.duration_ms = Some(250);
        }
        HookEvent::TaskFailed => {
            context.success = Some(false);
            context.error = Some("Sample hook failure".to_string());
            context.duration_ms = Some(250);
        }
        HookEvent::TaskInterrupted => {
            context.success = Some(false);
            context.error = Some("Sample hook interruption".to_string());
            context.duration_ms = Some(125);
        }
    }

    context
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{HookAction, HookEvent};
    use tempfile::tempdir;

    fn setup() -> (HookCapabilityService, tempfile::TempDir) {
        let temp_dir = tempdir().expect("temp dir");
        let db_path = temp_dir.path().join("hooks.db");
        let storage = Storage::new(db_path.to_str().expect("db path")).expect("storage");
        (HookCapabilityService::from_storage(&storage), temp_dir)
    }

    fn test_hook() -> Hook {
        Hook::new(
            "Notify".to_string(),
            HookEvent::TaskCompleted,
            HookAction::Webhook {
                url: "https://example.invalid/hook".to_string(),
                method: None,
                headers: None,
            },
        )
    }

    #[test]
    fn create_list_update_delete_round_trip() {
        let (service, _dir) = setup();
        let mut hook = test_hook();

        let created = service.create(hook.clone()).expect("create");
        assert_eq!(created.name, hook.name);
        assert_eq!(service.list().expect("list").len(), 1);

        hook.name = "Renamed".to_string();
        hook.touch();
        let updated = service.update(&created.id, hook.clone()).expect("update");
        assert_eq!(updated.name, "Renamed");

        assert!(service.delete(&created.id).expect("delete"));
        assert!(service.list().expect("list").is_empty());
    }

    #[test]
    fn sample_context_matches_started_event_shape() {
        let context = sample_hook_context(&HookEvent::TaskStarted);

        assert_eq!(context.event, HookEvent::TaskStarted);
        assert_eq!(context.success, None);
        assert_eq!(context.output, None);
        assert_eq!(context.error, None);
        assert_eq!(context.duration_ms, None);
    }

    #[test]
    fn sample_context_matches_failure_event_shape() {
        let context = sample_hook_context(&HookEvent::TaskFailed);

        assert_eq!(context.event, HookEvent::TaskFailed);
        assert_eq!(context.success, Some(false));
        assert_eq!(context.output, None);
        assert_eq!(context.error.as_deref(), Some("Sample hook failure"));
        assert_eq!(context.duration_ms, Some(250));
    }

    #[test]
    fn sample_context_matches_interrupted_event_shape() {
        let context = sample_hook_context(&HookEvent::TaskInterrupted);

        assert_eq!(context.event, HookEvent::TaskInterrupted);
        assert_eq!(context.success, Some(false));
        assert_eq!(context.output, None);
        assert_eq!(context.error.as_deref(), Some("Sample hook interruption"));
        assert_eq!(context.duration_ms, Some(125));
    }

}
