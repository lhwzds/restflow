use super::*;

fn parse_supported_hook_event(event_str: &str) -> Result<HookEvent, String> {
    serde_json::from_value(Value::String(event_str.to_string())).map_err(|_| {
        format!(
            "Invalid event: {}. Supported: task_started, task_completed, task_failed, task_interrupted",
            event_str
        )
    })
}

impl RestFlowMcpServer {
    pub(crate) async fn handle_manage_hooks(
        &self,
        params: ManageHooksParams,
    ) -> Result<String, String> {
        let operation = params.operation.trim().to_lowercase();

        let value = match operation.as_str() {
            "list" => {
                serde_json::to_value(self.backend.list_hooks().await?).map_err(|e| e.to_string())?
            }
            "create" => {
                let name = Self::required_string(params.name, "name")?;
                let event_str = Self::required_string(params.event, "event")?;
                let event = parse_supported_hook_event(&event_str)?;
                let action_value = params.action.ok_or("Missing required field: action")?;
                let action: HookAction = serde_json::from_value(action_value)
                    .map_err(|e| format!("Invalid action: {}", e))?;
                let mut hook = Hook::new(name, event, action);
                hook.description = params.description.flatten();
                if let Some(filter_value) = params.filter.flatten() {
                    hook.filter = Some(
                        serde_json::from_value::<HookFilter>(filter_value)
                            .map_err(|e| format!("Invalid filter: {}", e))?,
                    );
                }
                if let Some(enabled) = params.enabled {
                    hook.enabled = enabled;
                }
                serde_json::to_value(self.backend.create_hook(hook).await?)
                    .map_err(|e| e.to_string())?
            }
            "update" => {
                let id = Self::required_string(params.id, "id")?;
                let hooks = self.backend.list_hooks().await?;
                let mut hook = hooks
                    .into_iter()
                    .find(|h| h.id == id)
                    .ok_or_else(|| format!("Hook not found: {}", id))?;
                if let Some(name) = params.name {
                    hook.name = name;
                }
                if let Some(desc) = params.description {
                    hook.description = desc;
                }
                if let Some(event_str) = params.event {
                    hook.event = parse_supported_hook_event(&event_str)?;
                }
                if let Some(action_value) = params.action {
                    hook.action = serde_json::from_value(action_value)
                        .map_err(|e| format!("Invalid action: {}", e))?;
                }
                if let Some(filter_value) = params.filter {
                    hook.filter = match filter_value {
                        Some(filter_value) => Some(
                            serde_json::from_value::<HookFilter>(filter_value)
                                .map_err(|e| format!("Invalid filter: {}", e))?,
                        ),
                        None => None,
                    };
                }
                if let Some(enabled) = params.enabled {
                    hook.enabled = enabled;
                }
                hook.touch();
                serde_json::to_value(self.backend.update_hook(&id, hook).await?)
                    .map_err(|e| e.to_string())?
            }
            "delete" => {
                let id = Self::required_string(params.id, "id")?;
                let deleted = self.backend.delete_hook(&id).await?;
                serde_json::json!({ "id": id, "deleted": deleted })
            }
            "test" => {
                let id = Self::required_string(params.id, "id")?;
                self.backend.test_hook(&id).await?;
                serde_json::json!({ "id": id, "tested": true })
            }
            _ => {
                return Err(format!(
                    "Unknown operation: {}. Supported: list, create, update, delete, test",
                    operation
                ));
            }
        };

        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    }
}
