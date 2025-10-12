use anyhow::Result;
use std::path::PathBuf;

const DATA_DIR_NAME: &str = "restflow";
const DB_FILE_NAME: &str = "restflow.db";

/// Ensure the RestFlow data directory exists and return its path.
pub fn ensure_data_dir() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .or_else(|| dirs::home_dir())
        .ok_or_else(|| anyhow::anyhow!("Failed to determine system data directory"))?;
    let data_dir = base.join(DATA_DIR_NAME);
    std::fs::create_dir_all(&data_dir)?;
    Ok(data_dir)
}

/// Ensure the RestFlow database path can be used and return it.
pub fn ensure_database_path() -> Result<PathBuf> {
    Ok(ensure_data_dir()?.join(DB_FILE_NAME))
}

/// Convenience helper returning the database path as a UTF-8 string.
pub fn ensure_database_path_string() -> Result<String> {
    Ok(ensure_database_path()?.to_string_lossy().into_owned())
}
