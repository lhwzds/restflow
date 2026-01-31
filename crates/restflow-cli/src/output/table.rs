use anyhow::Result;
use comfy_table::Table;

#[allow(dead_code)]
pub fn print_table(table: Table) -> Result<()> {
    println!("{table}");
    Ok(())
}
