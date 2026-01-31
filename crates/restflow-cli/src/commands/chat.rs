use anyhow::Result;
use std::sync::Arc;

use crate::cli::ChatArgs;
use crate::tui;
use restflow_core::AppCore;

pub async fn run(core: Arc<AppCore>, _args: ChatArgs) -> Result<()> {
    tui::run(core).await
}
