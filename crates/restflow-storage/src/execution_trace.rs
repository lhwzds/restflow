use crate::define_simple_storage;

define_simple_storage! {
    /// Byte-level execution trace storage.
    pub struct ExecutionTraceStorage { store: "execution-trace" }
}
