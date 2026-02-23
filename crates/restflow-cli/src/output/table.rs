use anyhow::Result;
use comfy_table::Table;

pub fn print_table(table: Table) -> Result<()> {
    println!("{table}");
    Ok(())
}
