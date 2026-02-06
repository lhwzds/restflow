use clap::CommandFactory;
use clap_complete::{Shell, generate};

use crate::cli::Cli;

#[allow(dead_code)]
pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
}
