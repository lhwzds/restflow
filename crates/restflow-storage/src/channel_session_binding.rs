//! Channel session binding storage - byte-level API for channel/session mapping.
//!
//! Stores normalized bindings between channel conversation routes and workspace
//! chat session IDs.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level channel session binding storage with byte-level API.
    ///
    /// Key format is defined by restflow-core wrapper and should be stable:
    /// `{channel}:{account_or_star}:{conversation_id}`.
    pub struct ChannelSessionBindingStorage { table: "channel_session_bindings" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimpleStorage;
    use redb::Database;
    use std::sync::Arc;
    use tempfile::tempdir;

    #[test]
    fn test_put_get_delete_roundtrip() {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Arc::new(Database::create(db_path).unwrap());
        let storage = ChannelSessionBindingStorage::new(db).unwrap();

        let key = "telegram:*:chat-1";
        let value = br#"{"session_id":"sess-1"}"#;
        storage.put_raw(key, value).unwrap();

        let fetched = storage.get_raw(key).unwrap().unwrap();
        assert_eq!(fetched, value);

        let deleted = storage.delete(key).unwrap();
        assert!(deleted);
        assert!(storage.get_raw(key).unwrap().is_none());
    }
}
