use std::path::PathBuf;
use std::sync::Arc;

use crate::ToolRegistry;
use crate::impls::batch::BatchTool;
use crate::impls::edit::EditTool;
use crate::impls::glob_tool::GlobTool;
use crate::impls::grep_tool::GrepTool;
use crate::impls::multiedit::MultiEditTool;
use crate::impls::patch::PatchTool;
use types::store::DiagnosticsProvider;

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

    pub fn with_patch(mut self) -> Self {
        self.registry.register(PatchTool::new(self.tracker.clone()));
        self
    }

    pub fn with_patch_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
        let mut tool = PatchTool::new(self.tracker.clone()).require_base_dir();
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
        }
        self.registry.register(tool);
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

    pub fn with_edit_and_diagnostics_and_base_dir(
        mut self,
        diagnostics: Option<Arc<dyn DiagnosticsProvider>>,
        base_dir: Option<PathBuf>,
    ) -> Self {
        let mut tool = EditTool::with_tracker(self.tracker.clone()).require_base_dir();
        if let Some(diag) = diagnostics {
            tool = tool.with_diagnostics_provider(diag);
        }
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
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

    pub fn with_multiedit_and_diagnostics_and_base_dir(
        mut self,
        diagnostics: Option<Arc<dyn DiagnosticsProvider>>,
        base_dir: Option<PathBuf>,
    ) -> Self {
        let mut tool = MultiEditTool::with_tracker(self.tracker.clone()).require_base_dir();
        if let Some(diag) = diagnostics {
            tool = tool.with_diagnostics_provider(diag);
        }
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
        }
        self.registry.register(tool);
        self
    }

    pub fn with_glob(mut self) -> Self {
        self.registry.register(GlobTool::new());
        self
    }

    pub fn with_glob_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
        let mut tool = GlobTool::new().require_base_dir();
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
        }
        self.registry.register(tool);
        self
    }

    pub fn with_grep(mut self) -> Self {
        self.registry.register(GrepTool::new());
        self
    }

    pub fn with_grep_and_base_dir(mut self, base_dir: Option<PathBuf>) -> Self {
        let mut tool = GrepTool::new().require_base_dir();
        if let Some(base_dir) = base_dir {
            tool = tool.with_base_dir(base_dir);
        }
        self.registry.register(tool);
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
