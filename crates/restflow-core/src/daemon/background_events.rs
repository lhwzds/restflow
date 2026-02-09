use crate::runtime::TaskStreamEvent;
use std::sync::OnceLock;
use tokio::sync::broadcast;

const BUFFER_CAPACITY: usize = 512;

fn stream_sender() -> &'static broadcast::Sender<TaskStreamEvent> {
    static SENDER: OnceLock<broadcast::Sender<TaskStreamEvent>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, _receiver) = broadcast::channel(BUFFER_CAPACITY);
        sender
    })
}

/// Publish a background-agent stream event to daemon subscribers.
pub fn publish_background_event(event: TaskStreamEvent) {
    let _ = stream_sender().send(event);
}

/// Subscribe to the daemon background-agent stream bus.
pub fn subscribe_background_events() -> broadcast::Receiver<TaskStreamEvent> {
    stream_sender().subscribe()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_publish_and_subscribe_background_event() {
        let mut receiver = subscribe_background_events();
        let event = TaskStreamEvent::progress(
            "task-1",
            "notification",
            Some(42),
            Some("streaming".to_string()),
        );

        publish_background_event(event.clone());
        let received = receiver.recv().await.unwrap();

        assert_eq!(received.task_id, event.task_id);
    }
}
