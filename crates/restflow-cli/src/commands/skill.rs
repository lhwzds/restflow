use anyhow::{bail, Result};
use std::sync::Arc;

use crate::cli::SkillCommands;
use crate::output::OutputFormat;
use restflow_core::AppCore;

pub async fn run(_core: Arc<AppCore>, _command: SkillCommands, _format: OutputFormat) -> Result<()> {
    bail!("The skill command is not implemented yet.")
}
