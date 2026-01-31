use clap::CommandFactory;
use clap_mangen::Man;
use std::fs;
use std::path::PathBuf;

mod cli {
    include!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/cli.rs"));
}

fn main() {
    let out_dir = PathBuf::from("man");
    fs::create_dir_all(&out_dir).unwrap();

    let cmd = cli::Cli::command();

    let mut buffer = Vec::new();
    Man::new(cmd.clone()).render(&mut buffer).unwrap();
    fs::write(out_dir.join("restflow.1"), buffer).unwrap();

    for subcommand in cmd.get_subcommands() {
        let name = subcommand.get_name();
        let mut buffer = Vec::new();
        Man::new(subcommand.clone()).render(&mut buffer).unwrap();
        fs::write(out_dir.join(format!("restflow-{}.1", name)), buffer).unwrap();
    }
}
