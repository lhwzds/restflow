use crate::define_simple_storage;

define_simple_storage! {
    /// Byte-level structured execution log storage.
    pub struct StructuredExecutionLogStorage { table: "structured_execution_logs_v1" }
}
