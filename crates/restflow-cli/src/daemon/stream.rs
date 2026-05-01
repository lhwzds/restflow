use restflow_core::runtime::TaskEventEmitter;
use restflow_core::runtime::TaskStreamEvent;
use std::io::{self, Write};
use tokio::sync::Mutex;

pub struct CliEventEmitter {
    output: Mutex<io::Stdout>,
}

impl CliEventEmitter {
    pub fn new() -> Self {
        Self {
            output: Mutex::new(io::stdout()),
        }
    }
}

#[async_trait::async_trait]
impl TaskEventEmitter for CliEventEmitter {
    async fn emit(&self, event: TaskStreamEvent) {
        match serde_json::to_string(&event) {
            Ok(json) => {
                let mut output = self.output.lock().await;
                let _ = writeln!(output, "{}", json);
                let _ = output.flush();
            }
            Err(err) => {
                tracing::warn!("Failed to serialize task stream event: {}", err);
            }
        }
    }
}
