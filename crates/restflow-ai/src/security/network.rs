// Network ecosystem allowlist for tool security
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Network ecosystem categories for domain allowlists
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum NetworkEcosystem {
    /// Default safe domains (e.g., github.com, example.com)
    Defaults,
    /// Node.js ecosystem (npm, yarn)
    Node,
    /// Python ecosystem (PyPI)
    Python,
    /// Go ecosystem (go modules)
    Go,
    /// Rust ecosystem (crates.io)
    Rust,
    /// Custom domain list
    Custom(Vec<String>),
}

impl NetworkEcosystem {
    /// Get the allowed domains for this ecosystem
    pub fn allowed_domains(&self) -> Vec<String> {
        match self {
            NetworkEcosystem::Defaults => vec![
                "github.com".to_string(),
                "api.github.com".to_string(),
                "raw.githubusercontent.com".to_string(),
                "example.com".to_string(),
            ],
            NetworkEcosystem::Node => vec![
                "registry.npmjs.org".to_string(),
                "npmjs.com".to_string(),
                "yarnpkg.com".to_string(),
            ],
            NetworkEcosystem::Python => {
                vec!["pypi.org".to_string(), "files.pythonhosted.org".to_string()]
            }
            NetworkEcosystem::Go => vec![
                "proxy.golang.org".to_string(),
                "go.dev".to_string(),
                "pkg.go.dev".to_string(),
            ],
            NetworkEcosystem::Rust => vec!["crates.io".to_string(), "static.crates.io".to_string()],
            NetworkEcosystem::Custom(domains) => domains.clone(),
        }
    }
}

/// Network allowlist configuration for background agents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkAllowlist {
    /// List of allowed network ecosystems
    pub ecosystems: Vec<NetworkEcosystem>,
}

impl NetworkAllowlist {
    /// Create a new network allowlist
    pub fn new(ecosystems: Vec<NetworkEcosystem>) -> Self {
        Self { ecosystems }
    }

    /// Check if a host is allowed
    pub fn is_host_allowed(&self, host: &str) -> bool {
        // Flatten all allowed domains
        let allowed: HashSet<String> = self
            .ecosystems
            .iter()
            .flat_map(|eco| eco.allowed_domains())
            .collect();

        // Check exact match or subdomain match
        allowed.contains(host)
            || allowed.iter().any(|domain| {
                host.ends_with(&format!(".{}", domain)) || domain.ends_with(&format!(".{}", host))
            })
    }

    /// Get all allowed domains
    pub fn allowed_domains(&self) -> Vec<String> {
        self.ecosystems
            .iter()
            .flat_map(|eco| eco.allowed_domains())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_ecosystem() {
        let domains = NetworkEcosystem::Defaults.allowed_domains();
        assert!(domains.contains(&"github.com".to_string()));
        assert!(domains.contains(&"api.github.com".to_string()));
    }

    #[test]
    fn test_node_ecosystem() {
        let domains = NetworkEcosystem::Node.allowed_domains();
        assert!(domains.contains(&"registry.npmjs.org".to_string()));
        assert!(domains.contains(&"npmjs.com".to_string()));
    }

    #[test]
    fn test_python_ecosystem() {
        let domains = NetworkEcosystem::Python.allowed_domains();
        assert!(domains.contains(&"pypi.org".to_string()));
        assert!(domains.contains(&"files.pythonhosted.org".to_string()));
    }

    #[test]
    fn test_go_ecosystem() {
        let domains = NetworkEcosystem::Go.allowed_domains();
        assert!(domains.contains(&"proxy.golang.org".to_string()));
        assert!(domains.contains(&"go.dev".to_string()));
    }

    #[test]
    fn test_rust_ecosystem() {
        let domains = NetworkEcosystem::Rust.allowed_domains();
        assert!(domains.contains(&"crates.io".to_string()));
        assert!(domains.contains(&"static.crates.io".to_string()));
    }

    #[test]
    fn test_custom_ecosystem() {
        let domains =
            NetworkEcosystem::Custom(vec!["custom.com".to_string(), "example.org".to_string()])
                .allowed_domains();
        assert_eq!(domains.len(), 2);
        assert!(domains.contains(&"custom.com".to_string()));
        assert!(domains.contains(&"example.org".to_string()));
    }

    #[test]
    fn test_allowlist_is_host_allowed() {
        let allowlist =
            NetworkAllowlist::new(vec![NetworkEcosystem::Defaults, NetworkEcosystem::Python]);

        // Exact match
        assert!(allowlist.is_host_allowed("github.com"));
        assert!(allowlist.is_host_allowed("pypi.org"));

        // Subdomain match
        assert!(allowlist.is_host_allowed("api.github.com"));
        assert!(allowlist.is_host_allowed("files.pythonhosted.org"));

        // Not allowed
        assert!(!allowlist.is_host_allowed("npmjs.com"));
        assert!(!allowlist.is_host_allowed("crates.io"));
    }

    #[test]
    fn test_allowlist_allowed_domains() {
        let allowlist =
            NetworkAllowlist::new(vec![NetworkEcosystem::Defaults, NetworkEcosystem::Node]);

        let domains = allowlist.allowed_domains();
        assert!(domains.contains(&"github.com".to_string()));
        assert!(domains.contains(&"registry.npmjs.org".to_string()));
    }
}
