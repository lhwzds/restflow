use anyhow::{Result, bail};
use comfy_table::{Cell, Table};
use runtime::session_import::{ImportOptions, ImportSource, import_sessions};
use runtime::session_log::FileSessionStore;

use crate::cli::{ImportArgs, ImportSourceArg};
use crate::output::{OutputFormat, json::print_json};

pub fn run(args: ImportArgs, format: OutputFormat) -> Result<()> {
    if args.source == ImportSourceArg::All && args.path.is_some() {
        bail!("--path can only be used with a single import source");
    }

    let store = FileSessionStore::open_default()?;
    let report = import_sessions(
        &store,
        ImportOptions {
            source: args.source.into(),
            path: args.path,
            dry_run: args.dry_run,
            force: args.force,
        },
    )?;

    if format.is_json() {
        return print_json(&report);
    }

    let mut table = Table::new();
    table.set_header(vec![
        "Source",
        "Discovered",
        if args.dry_run {
            "Would Import"
        } else {
            "Imported"
        },
        "Skipped",
        "Failed",
    ]);
    for source in &report.sources {
        table.add_row(vec![
            Cell::new(&source.source),
            Cell::new(source.discovered),
            Cell::new(source.imported),
            Cell::new(source.skipped),
            Cell::new(source.failed),
        ]);
    }
    crate::output::table::print_table(table)?;
    if !report
        .sources
        .iter()
        .any(|source| !source.errors.is_empty())
    {
        return Ok(());
    }

    println!();
    for source in &report.sources {
        for error in &source.errors {
            println!("{}: {}", source.source, error);
        }
    }
    Ok(())
}

impl From<ImportSourceArg> for ImportSource {
    fn from(value: ImportSourceArg) -> Self {
        match value {
            ImportSourceArg::All => Self::All,
            ImportSourceArg::Claude => Self::Claude,
            ImportSourceArg::Codex => Self::Codex,
            ImportSourceArg::Opencode => Self::Opencode,
        }
    }
}
