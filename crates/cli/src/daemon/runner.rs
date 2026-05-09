use anyhow::Result;
use runtime::AppCore;
use std::sync::Arc;
use tracing::info;

pub struct CliTaskRunner {
    _core: Arc<AppCore>,
}

impl CliTaskRunner {
    pub fn new(core: Arc<AppCore>) -> Self {
        Self { _core: core }
    }

    pub async fn start(&mut self) -> Result<()> {
        info!("Legacy task runner disabled; daemon only hosts background services");
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        Ok(())
    }
}
