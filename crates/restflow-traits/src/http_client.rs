//! HTTP client utilities for tool implementations.

use reqwest::Client;
use reqwest::redirect::Policy;
use std::net::SocketAddr;

const DISABLE_SYSTEM_PROXY_ENV: &str = "RESTFLOW_DISABLE_SYSTEM_PROXY";

/// Build a standard HTTP client respecting proxy settings.
pub fn build_http_client() -> Result<Client, reqwest::Error> {
    if should_disable_system_proxy() {
        Client::builder().no_proxy().build()
    } else {
        Client::builder().build()
    }
}

/// Build an SSRF-safe HTTP client that pins DNS to a pre-validated IP
/// and disables automatic redirects (caller must handle redirect loop).
pub fn build_ssrf_safe_client(host: &str, addr: SocketAddr) -> Result<Client, reqwest::Error> {
    let mut builder = Client::builder()
        .redirect(Policy::none())
        .resolve(host, addr);

    if should_disable_system_proxy() {
        builder = builder.no_proxy();
    }

    builder.build()
}

fn should_disable_system_proxy() -> bool {
    if std::env::var_os(DISABLE_SYSTEM_PROXY_ENV).is_some() {
        return true;
    }

    cfg!(test)
}
