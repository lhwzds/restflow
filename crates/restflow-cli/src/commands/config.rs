use anyhow::{bail, Result};
use std::sync::Arc;

use crate::cli::ConfigCommands;
use crate::output::OutputFormat;
use restflow_core::AppCore;

pub async fn run(_core: Arc<AppCore>, _command: ConfigCommands, _format: OutputFormat) -> Result<()> {
    bail!("The config command is not implemented yet.")
}
