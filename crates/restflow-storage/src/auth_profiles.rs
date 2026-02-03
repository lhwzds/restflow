//! Auth profile storage - byte-level API for auth profile persistence.

use crate::define_simple_storage;

define_simple_storage! {
    /// Low-level auth profile storage with byte-level API.
    pub struct AuthProfileStorage { table: "auth_profiles" }
}
