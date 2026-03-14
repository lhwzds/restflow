use std::sync::Arc;

use crate::impls::batch::BatchTool;
use crate::impls::browser::BrowserTool;
use crate::impls::edit::EditTool;
use crate::impls::glob_tool::GlobTool;
use crate::impls::grep_tool::GrepTool;
use crate::impls::jina_reader::JinaReaderTool;
use crate::impls::monty_python::{PythonTool, RunPythonTool};
use crate::impls::multiedit::MultiEditTool;
use crate::impls::patch::PatchTool;
use crate::impls::transcribe::TranscribeTool;
use crate::impls::vision::VisionTool;
use crate::impls::web_fetch::WebFetchTool;
use crate::impls::web_search::WebSearchTool;
use crate::impls::{DiscordTool, EmailTool, HttpTool, SlackTool, TelegramTool};
use crate::{SecretResolver, ToolRegistry};
use restflow_traits::store::DiagnosticsProvider;

use super::ToolRegistryBuilder;
use super::configs::{BashConfig, FileConfig};

impl ToolRegistryBuilder {
    pub fn with_bash(mut self, config: BashConfig) -> Self {
        self.registry.register(config.into_bash_tool());
        self
    }

    pub fn with_file(mut self, config: FileConfig) -> Self {
        self.registry
            .register(config.into_file_tool_with_tracker(self.tracker.clone()));
        self
    }

    pub fn with_http(mut self) -> std::result::Result<Self, reqwest::Error> {
        self.registry.register(HttpTool::new()?);
        Ok(self)
    }

    pub fn with_email(mut self) -> Self {
        self.registry.register(EmailTool::new());
        self
    }

    pub fn with_telegram(mut self) -> std::result::Result<Self, reqwest::Error> {
        self.registry.register(TelegramTool::new()?);
        Ok(self)
    }

    pub fn with_discord(mut self) -> std::result::Result<Self, reqwest::Error> {
        self.registry.register(DiscordTool::new()?);
        Ok(self)
    }

    pub fn with_slack(mut self) -> std::result::Result<Self, reqwest::Error> {
        self.registry.register(SlackTool::new()?);
        Ok(self)
    }

    pub fn with_python(mut self) -> Self {
        self.registry.register(RunPythonTool::new());
        self.registry.register(PythonTool::new());
        self
    }

    pub fn with_browser(mut self) -> anyhow::Result<Self> {
        self.registry.register(BrowserTool::new()?);
        Ok(self)
    }

    pub fn with_browser_timeout(mut self, timeout_secs: u64) -> anyhow::Result<Self> {
        self.registry
            .register(BrowserTool::new_with_timeout(timeout_secs)?);
        Ok(self)
    }

    pub fn with_transcribe(
        mut self,
        resolver: SecretResolver,
    ) -> std::result::Result<Self, reqwest::Error> {
        self.registry.register(TranscribeTool::new(resolver)?);
        Ok(self)
    }

    pub fn with_vision(
        mut self,
        resolver: SecretResolver,
    ) -> std::result::Result<Self, reqwest::Error> {
        self.registry.register(VisionTool::new(resolver)?);
        Ok(self)
    }

    pub fn with_web_fetch(mut self) -> Self {
        self.registry.register(WebFetchTool::new());
        self
    }

    pub fn with_jina_reader(mut self) -> std::result::Result<Self, reqwest::Error> {
        self.registry.register(JinaReaderTool::new()?);
        Ok(self)
    }

    pub fn with_web_search(mut self) -> std::result::Result<Self, reqwest::Error> {
        self.registry.register(WebSearchTool::new()?);
        Ok(self)
    }

    pub fn with_web_search_with_defaults(
        mut self,
        default_num_results: usize,
    ) -> std::result::Result<Self, reqwest::Error> {
        self.registry
            .register(WebSearchTool::with_default_num_results(
                default_num_results,
            )?);
        Ok(self)
    }

    pub fn with_web_search_with_resolver(
        mut self,
        resolver: SecretResolver,
    ) -> std::result::Result<Self, reqwest::Error> {
        self.registry
            .register(WebSearchTool::new()?.with_secret_resolver(resolver));
        Ok(self)
    }

    pub fn with_web_search_with_resolver_and_defaults(
        mut self,
        resolver: SecretResolver,
        default_num_results: usize,
    ) -> std::result::Result<Self, reqwest::Error> {
        self.registry.register(
            WebSearchTool::with_default_num_results(default_num_results)?
                .with_secret_resolver(resolver),
        );
        Ok(self)
    }

    pub fn with_patch(mut self) -> Self {
        self.registry.register(PatchTool::new(self.tracker.clone()));
        self
    }

    pub fn with_edit(self) -> Self {
        self.with_edit_and_diagnostics(None)
    }

    pub fn with_edit_and_diagnostics(
        mut self,
        diagnostics: Option<Arc<dyn DiagnosticsProvider>>,
    ) -> Self {
        let mut tool = EditTool::with_tracker(self.tracker.clone());
        if let Some(diag) = diagnostics {
            tool = tool.with_diagnostics_provider(diag);
        }
        self.registry.register(tool);
        self
    }

    pub fn with_multiedit(self) -> Self {
        self.with_multiedit_and_diagnostics(None)
    }

    pub fn with_multiedit_and_diagnostics(
        mut self,
        diagnostics: Option<Arc<dyn DiagnosticsProvider>>,
    ) -> Self {
        let mut tool = MultiEditTool::with_tracker(self.tracker.clone());
        if let Some(diag) = diagnostics {
            tool = tool.with_diagnostics_provider(diag);
        }
        self.registry.register(tool);
        self
    }

    pub fn with_glob(mut self) -> Self {
        self.registry.register(GlobTool::new());
        self
    }

    pub fn with_grep(mut self) -> Self {
        self.registry.register(GrepTool::new());
        self
    }

    /// Register the batch tool. This requires an `Arc<ToolRegistry>` containing
    /// the tools the batch tool can invoke. Typically used in a two-phase build:
    /// 1. Build the base registry with `build()` and wrap in `Arc`
    /// 2. Register the batch tool on the Arc'd registry
    ///
    /// Alternatively, use `build_with_batch()` which handles this automatically.
    pub fn with_batch(mut self, tools: Arc<ToolRegistry>) -> Self {
        self.registry.register(BatchTool::new(tools));
        self
    }
}
