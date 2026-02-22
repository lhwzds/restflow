// Network ecosystem allowlist for tool security
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};

/// Validate URL to prevent SSRF attacks.
/// Blocks access to internal/private network resources.
pub fn validate_url(url: &str) -> std::result::Result<(), String> {
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;

    // Only allow HTTP and HTTPS schemes
    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "Scheme '{}' is not allowed. Only HTTP and HTTPS are permitted.",
                scheme
            ));
        }
    }

    // Check host
    let host = match parsed.host_str() {
        Some(h) => h,
        None => return Err("URL must have a host".to_string()),
    };

    // Block localhost variations
    if host.eq_ignore_ascii_case("localhost")
        || host == "0.0.0.0"
        || host == "::1"
        || host == "[::1]"
    {
        return Err("Access to localhost is not allowed".to_string());
    }

    // Try to parse as IP address
    if let Ok(ip) = host.parse::<IpAddr>()
        && is_restricted_ip(&ip)
    {
        return Err(format!(
            "Access to restricted IP address {} is not allowed (private/internal/metadata)",
            ip
        ));
    }

    // Handle bracketed IPv6 addresses
    if host.starts_with('[') && host.ends_with(']') {
        let inner = &host[1..host.len() - 1];
        if let Ok(ip) = inner.parse::<Ipv6Addr>()
            && is_restricted_ip(&IpAddr::V6(ip))
        {
            return Err(format!(
                "Access to restricted IPv6 address {} is not allowed",
                ip
            ));
        }
    }

    Ok(())
}

/// Resolve DNS and validate all resolved IPs against SSRF restrictions.
///
/// Performs both string-level validation (scheme, literal IP checks) and
/// DNS resolution to catch domain names that resolve to private addresses.
/// Returns the parsed URL and a validated SocketAddr for IP pinning.
pub async fn resolve_and_validate_url(
    url_str: &str,
) -> std::result::Result<(url::Url, SocketAddr), String> {
    // First pass: scheme + literal IP checks
    validate_url(url_str)?;

    let parsed = url::Url::parse(url_str).map_err(|e| format!("Invalid URL: {}", e))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL must have a host".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(443);

    // Resolve DNS and check ALL resolved IPs
    let addr_str = format!("{}:{}", host, port);
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(&addr_str)
        .await
        .map_err(|e| format!("DNS resolution failed for '{}': {}", host, e))?
        .collect();

    if addrs.is_empty() {
        return Err(format!("DNS resolved zero addresses for '{}'", host));
    }

    // Every resolved IP must pass the restricted check
    for addr in &addrs {
        if is_restricted_ip(&addr.ip()) {
            return Err(format!(
                "DNS for '{}' resolved to restricted IP {} (private/internal/metadata)",
                host,
                addr.ip()
            ));
        }
    }

    // Return first valid address for IP pinning
    Ok((parsed, addrs[0]))
}

/// Check if an IP address is in a restricted range.
pub fn is_restricted_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            // Loopback: 127.0.0.0/8
            if v4.is_loopback() {
                return true;
            }

            // Private ranges
            if v4.is_private() {
                return true;
            }

            // Link-local: 169.254.0.0/16 (includes AWS metadata 169.254.169.254)
            if v4.is_link_local() {
                return true;
            }

            // Broadcast: 255.255.255.255
            if v4.is_broadcast() {
                return true;
            }

            // Documentation: 192.0.2.0/24, 198.51.100.0/24, 203.0.113.0/24
            if v4.is_documentation() {
                return true;
            }

            // Shared address space: 100.64.0.0/10 (CGNAT)
            if matches!(v4.octets(), [100, 64..=127, ..]) {
                return true;
            }

            // IETF Protocol Assignments: 192.0.0.0/24
            if matches!(v4.octets(), [192, 0, 0, _]) {
                return true;
            }

            // Benchmark testing: 198.18.0.0/15
            if matches!(v4.octets(), [198, 18..=19, ..]) {
                return true;
            }

            // Multicast: 224.0.0.0/4
            if v4.is_multicast() {
                return true;
            }

            // Reserved for future use: 240.0.0.0/4
            if matches!(v4.octets(), [240..=255, ..]) {
                return true;
            }

            false
        }
        IpAddr::V6(v6) => {
            // Loopback: ::1
            if v6.is_loopback() {
                return true;
            }

            // Unique local (like private): fc00::/7
            if matches!(v6.segments(), [0xfc00..=0xfdff, ..]) {
                return true;
            }

            // Link-local: fe80::/10
            if matches!(v6.segments(), [0xfe80..=0xfebf, ..]) {
                return true;
            }

            // Multicast: ff00::/8
            if v6.is_multicast() {
                return true;
            }

            // Documentation: 2001:db8::/32
            if matches!(v6.segments(), [0x2001, 0x0db8, ..]) {
                return true;
            }

            false
        }
    }
}

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
                host.ends_with(&format!(".{}", domain))
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

        // Public suffix bypass: bare TLDs must not match
        assert!(!allowlist.is_host_allowed("io"));
        assert!(!allowlist.is_host_allowed("com"));
        assert!(!allowlist.is_host_allowed("org"));
    }

    #[test]
    fn test_allowlist_allowed_domains() {
        let allowlist =
            NetworkAllowlist::new(vec![NetworkEcosystem::Defaults, NetworkEcosystem::Node]);

        let domains = allowlist.allowed_domains();
        assert!(domains.contains(&"github.com".to_string()));
        assert!(domains.contains(&"registry.npmjs.org".to_string()));
    }

    #[test]
    fn test_validate_url_localhost_blocked() {
        assert!(validate_url("http://localhost/admin").is_err());
        assert!(validate_url("http://127.0.0.1/admin").is_err());
        assert!(validate_url("http://0.0.0.0/admin").is_err());
        assert!(validate_url("http://[::1]/admin").is_err());
    }

    #[test]
    fn test_validate_url_private_ip_blocked() {
        assert!(validate_url("http://10.0.0.1/").is_err());
        assert!(validate_url("http://172.16.0.1/").is_err());
        assert!(validate_url("http://192.168.1.1/").is_err());
    }

    #[test]
    fn test_validate_url_link_local_blocked() {
        assert!(validate_url("http://169.254.169.254/latest/meta-data/").is_err());
    }

    #[test]
    fn test_validate_url_invalid_scheme_blocked() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("ftp://example.com/file").is_err());
    }

    #[test]
    fn test_validate_url_public_allowed() {
        assert!(validate_url("https://example.com/").is_ok());
        assert!(validate_url("https://api.github.com/").is_ok());
        assert!(validate_url("http://8.8.8.8/").is_ok());
    }

    #[test]
    fn test_validate_url_multicast_blocked() {
        assert!(validate_url("http://224.0.0.1/").is_err());
    }

    #[tokio::test]
    async fn test_resolve_and_validate_rejects_localhost() {
        assert!(resolve_and_validate_url("http://127.0.0.1/").await.is_err());
        assert!(resolve_and_validate_url("http://[::1]/").await.is_err());
        assert!(resolve_and_validate_url("http://0.0.0.0/").await.is_err());
    }

    #[tokio::test]
    async fn test_resolve_and_validate_rejects_private_ip() {
        assert!(resolve_and_validate_url("http://10.0.0.1/").await.is_err());
        assert!(resolve_and_validate_url("http://192.168.1.1/").await.is_err());
        assert!(resolve_and_validate_url("http://169.254.169.254/").await.is_err());
    }

    #[tokio::test]
    async fn test_resolve_and_validate_rejects_bad_scheme() {
        assert!(resolve_and_validate_url("ftp://example.com/").await.is_err());
        assert!(resolve_and_validate_url("file:///etc/passwd").await.is_err());
    }

    #[tokio::test]
    async fn test_resolve_and_validate_returns_socket_addr() {
        let result = resolve_and_validate_url("https://example.com/").await;
        match result {
            Ok((url, addr)) => {
                assert_eq!(url.host_str(), Some("example.com"));
                assert!(!is_restricted_ip(&addr.ip()));
            }
            Err(_) => {
                // DNS may fail in CI/offline environments — that's acceptable
            }
        }
    }
}
