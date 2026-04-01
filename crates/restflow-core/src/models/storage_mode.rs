use serde::{Deserialize, Serialize};
use specta::Type;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, Type, PartialEq, Eq, Default)]
#[specta(skip_attr = "ts")]
#[ts(export)]
pub enum StorageMode {
    #[default]
    DatabaseOnly,
    FileSystemOnly,
    Hybrid,
}
