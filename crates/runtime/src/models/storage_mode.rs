use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq, Eq, Default)]
pub enum StorageMode {
    #[default]
    DatabaseOnly,
    FileSystemOnly,
    Hybrid,
}
