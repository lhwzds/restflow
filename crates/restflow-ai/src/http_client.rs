use reqwest::Client;

const DISABLE_SYSTEM_PROXY_ENV: &str = "RESTFLOW_DISABLE_SYSTEM_PROXY";

pub(crate) fn build_http_client() -> Client {
    if should_disable_system_proxy() {
        Client::builder()
            .no_proxy()
            .build()
            .expect("Failed to build reqwest client")
    } else {
        Client::new()
    }
}

fn should_disable_system_proxy() -> bool {
    if std::env::var_os(DISABLE_SYSTEM_PROXY_ENV).is_some() {
        return true;
    }

    cfg!(test)
}
