use anyhow::Result;
use serde::Serialize;

#[allow(dead_code)]
pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let output = serde_json::to_string_pretty(value)?;
    println!("{output}");
    Ok(())
}
