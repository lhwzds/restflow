use anyhow::{bail, Result};
use std::sync::Arc;

use restflow_core::AppCore;

pub async fn run(_core: Arc<AppCore>) -> Result<()> {
    bail!("The MCP command is not implemented yet.")
}
