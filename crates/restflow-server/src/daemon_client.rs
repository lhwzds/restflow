use axum::http::{HeaderMap, Method};
use reqwest::Response;

#[derive(Clone)]
pub struct DaemonClient {
    http_client: reqwest::Client,
    daemon_url: String,
}

impl DaemonClient {
    pub fn new(daemon_url: &str) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            daemon_url: daemon_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn forward(
        &self,
        method: Method,
        path: &str,
        headers: HeaderMap,
        body: Option<Vec<u8>>,
    ) -> anyhow::Result<Response> {
        let url = format!("{}{}", self.daemon_url, path);
        let mut request = self.http_client.request(method, url);

        for (key, value) in headers.iter() {
            request = request.header(key, value);
        }

        if let Some(body) = body {
            request = request.body(body);
        }

        let response = request.send().await?;
        Ok(response)
    }

    pub fn ws_url(&self, path: &str) -> String {
        if self.daemon_url.starts_with("https://") {
            format!("wss://{}{}", &self.daemon_url[8..], path)
        } else if self.daemon_url.starts_with("http://") {
            format!("ws://{}{}", &self.daemon_url[7..], path)
        } else {
            format!("ws://{}{}", self.daemon_url, path)
        }
    }
}
