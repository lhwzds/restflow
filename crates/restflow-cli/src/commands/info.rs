use anyhow::Result;

pub fn run() -> Result<()> {
    println!("浮流 RestFlow CLI {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
