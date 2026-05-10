mod anthropic;
mod deepseek;
mod openai;

use reqwest::Client;

pub use anthropic::AnthropicClient;
pub use deepseek::DeepSeekClient;
pub use openai::OpenAIClient;

const DISABLE_SYSTEM_PROXY_ENV: &str = "RESTFLOW_DISABLE_SYSTEM_PROXY";

fn build_http_client() -> Result<Client, reqwest::Error> {
    if should_disable_system_proxy() {
        Client::builder().no_proxy().build()
    } else {
        Client::builder().build()
    }
}

fn should_disable_system_proxy() -> bool {
    if std::env::var_os(DISABLE_SYSTEM_PROXY_ENV).is_some() {
        return true;
    }

    cfg!(test)
}
