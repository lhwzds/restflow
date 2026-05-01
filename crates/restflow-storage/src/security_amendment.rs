use crate::define_simple_storage;

define_simple_storage! {
    /// Byte-level security amendment storage.
    pub struct SecurityAmendmentStorage { table: "security_amendments" }
}
