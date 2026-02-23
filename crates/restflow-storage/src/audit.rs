use crate::define_simple_storage;

define_simple_storage! {
    /// Byte-level audit event storage.
    pub struct AuditStorage { table: "audit_events_v2" }
}
