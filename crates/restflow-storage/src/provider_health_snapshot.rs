use crate::define_simple_storage;

define_simple_storage! {
    /// Byte-level provider health snapshot storage.
    pub struct ProviderHealthSnapshotStorage { table: "provider_health_snapshots_v1" }
}
