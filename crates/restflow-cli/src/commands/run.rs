use anyhow::{bail, Result};
use std::sync::Arc;

use crate::cli::RunArgs;
use crate::output::OutputFormat;
use restflow_core::AppCore;

pub async fn run(_core: Arc<AppCore>, _args: RunArgs, _format: OutputFormat) -> Result<()> {
    bail!("The run command is not implemented yet.")
}
