use anyhow::Result;

pub fn run() -> Result<()> {
    println!("RestFlow CLI {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
