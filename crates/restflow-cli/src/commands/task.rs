use anyhow::{bail, Result};
use std::sync::Arc;

use crate::cli::TaskCommands;
use crate::output::OutputFormat;
use restflow_core::AppCore;

pub async fn run(_core: Arc<AppCore>, _command: TaskCommands, _format: OutputFormat) -> Result<()> {
    bail!("The task command is not implemented yet.")
}
