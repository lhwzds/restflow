use clap::CommandFactory;
use clap_complete::{generate, Shell};

use crate::cli::Cli;

pub fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    generate(shell, &mut cmd, name, &mut std::io::stdout());
}
