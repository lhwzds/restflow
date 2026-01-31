use anyhow::{bail, Result};
use std::sync::Arc;

use crate::cli::SecretCommands;
use crate::output::OutputFormat;
use restflow_core::AppCore;

pub async fn run(_core: Arc<AppCore>, _command: SecretCommands, _format: OutputFormat) -> Result<()> {
    bail!("The secret command is not implemented yet.")
}
