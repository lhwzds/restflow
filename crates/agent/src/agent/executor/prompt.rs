use super::{AgentConfig, AgentExecutor};

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
}
