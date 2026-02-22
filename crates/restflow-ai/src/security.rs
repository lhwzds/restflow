//! Security abstractions — re-exported from restflow-traits.

pub use restflow_traits::security::{SecurityDecision, SecurityGate, ToolAction};

// Network security types re-exported under `security::network` for backward compat
pub mod network {
    pub use restflow_traits::network::{
        NetworkAllowlist, NetworkEcosystem, is_restricted_ip, resolve_and_validate_url,
        validate_url,
    };
}

pub use network::{
    NetworkAllowlist, NetworkEcosystem, is_restricted_ip, resolve_and_validate_url, validate_url,
};
