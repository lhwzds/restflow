use crate::agent::state::AgentState;
use crate::error::Result;
use tracing::debug;

use super::{AgentConfig, AgentExecutor, CheckpointDurability};

impl AgentExecutor {
    pub(crate) async fn build_system_prompt(&self, config: &AgentConfig) -> String {
        let mut sections = Vec::new();
        let flags = &config.prompt_flags;

        // Base prompt section (identity, role)
        if flags.include_base {
            let base = config
                .system_prompt
                .as_deref()
                .unwrap_or(crate::agent::DEFAULT_AGENT_PROMPT);
            sections.push(base.to_string());
        }

        // Tools section
        if flags.include_tools {
            let tools_desc: Vec<String> = self
                .tools
                .list()
                .iter()
                .filter_map(|name| self.tools.get(name))
                .map(|t| format!("- {}: {}", t.name(), t.description()))
                .collect();

            if !tools_desc.is_empty() {
                sections.push(format!("## Available Tools\n\n{}", tools_desc.join("\n")));
            }
        }

        // Workspace context section
        if flags.include_workspace_context
            && let Some(cache) = &self.context_cache
        {
            let context = cache.get().await;
            if !context.content.is_empty() {
                debug!(
                    files = ?context.loaded_files,
                    bytes = context.total_bytes,
                    "Loaded workspace context"
                );
                sections.push(context.content.clone());
            }
        }
        // Agent context section (skills, memory summary)
        if flags.include_agent_context
            && config.inject_agent_context
            && let Some(ref context) = config.agent_context
        {
            let context_str = context.format_for_prompt();
            if !context_str.is_empty() {
                sections.push(context_str);
            }
        }

        // Security policy section (placeholder for future integration)
        // When XPIA Security Policy is implemented, this section will be populated
        // from the security module based on flags.include_security_policy

        sections.join("\n\n")
    }

    pub(crate) async fn maybe_checkpoint(
        &self,
        config: &AgentConfig,
        state: &AgentState,
        terminal: bool,
    ) -> Result<()> {
        let Some(callback) = &config.checkpoint_callback else {
            return Ok(());
        };
        let should_checkpoint = if terminal {
            matches!(
                config.checkpoint_durability,
                CheckpointDurability::OnComplete
            )
        } else {
            match config.checkpoint_durability {
                CheckpointDurability::PerTurn => true,
                CheckpointDurability::Periodic { interval } => {
                    let interval = interval.max(1);
                    state.iteration > 0 && state.iteration.is_multiple_of(interval)
                }
                CheckpointDurability::OnComplete => false,
            }
        };
        if should_checkpoint {
            callback(state).await?;
        }
        Ok(())
    }
}
