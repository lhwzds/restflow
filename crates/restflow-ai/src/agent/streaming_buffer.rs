use std::collections::HashMap;
use std::time::{Duration, Instant};

const DEFAULT_FLUSH_INTERVAL_MS: u64 = 300;
const DEFAULT_CHUNK_THRESHOLD: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BufferMode {
    Accumulate,
    Replace,
}

#[derive(Debug)]
struct BufferEntry {
    content: String,
    chunk_count: usize,
    last_flush: Instant,
}

#[derive(Debug)]
pub struct StreamingBuffer {
    buffers: HashMap<String, BufferEntry>,
    flush_interval: Duration,
    chunk_threshold: usize,
}

impl Default for StreamingBuffer {
    fn default() -> Self {
        Self::new(
            Duration::from_millis(DEFAULT_FLUSH_INTERVAL_MS),
            DEFAULT_CHUNK_THRESHOLD,
        )
    }
}

impl StreamingBuffer {
    pub fn new(flush_interval: Duration, chunk_threshold: usize) -> Self {
        Self {
            buffers: HashMap::new(),
            flush_interval,
            chunk_threshold,
        }
    }

    pub fn append(&mut self, id: &str, chunk: &str, mode: BufferMode) -> Option<String> {
        let now = Instant::now();
        let entry = self
            .buffers
            .entry(id.to_string())
            .or_insert_with(|| BufferEntry {
                content: String::new(),
                chunk_count: 0,
                last_flush: now,
            });

        match mode {
            BufferMode::Accumulate => entry.content.push_str(chunk),
            BufferMode::Replace => entry.content = chunk.to_string(),
        }
        entry.chunk_count += 1;

        if entry.chunk_count >= self.chunk_threshold
            || now.duration_since(entry.last_flush) >= self.flush_interval
        {
            return self.flush(id);
        }

        None
    }

    pub fn flush(&mut self, id: &str) -> Option<String> {
        let now = Instant::now();
        let entry = self.buffers.get_mut(id)?;
        if entry.content.is_empty() {
            entry.chunk_count = 0;
            entry.last_flush = now;
            return None;
        }

        let content = std::mem::take(&mut entry.content);
        entry.chunk_count = 0;
        entry.last_flush = now;
        Some(content)
    }

    pub fn flush_all(&mut self) -> Vec<(String, String)> {
        let keys: Vec<String> = self.buffers.keys().cloned().collect();
        keys.into_iter()
            .filter_map(|id| self.flush(&id).map(|content| (id, content)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn flushes_on_chunk_threshold() {
        let mut buffer = StreamingBuffer::new(Duration::from_secs(60), 2);
        assert_eq!(buffer.append("exec-1", "a", BufferMode::Accumulate), None);
        assert_eq!(
            buffer.append("exec-1", "b", BufferMode::Accumulate),
            Some("ab".to_string())
        );
    }

    #[test]
    fn flushes_on_time_interval() {
        let mut buffer = StreamingBuffer::new(Duration::from_millis(1), 100);
        assert_eq!(buffer.append("exec-1", "a", BufferMode::Accumulate), None);
        sleep(Duration::from_millis(2));
        assert_eq!(
            buffer.append("exec-1", "b", BufferMode::Accumulate),
            Some("ab".to_string())
        );
    }

    #[test]
    fn replace_mode_overwrites_previous_content() {
        let mut buffer = StreamingBuffer::new(Duration::from_secs(60), 10);
        assert_eq!(
            buffer.append("exec-1", "hello", BufferMode::Accumulate),
            None
        );
        assert_eq!(buffer.append("exec-1", "world", BufferMode::Replace), None);
        assert_eq!(buffer.flush("exec-1"), Some("world".to_string()));
    }

    #[test]
    fn flush_all_returns_all_pending_items() {
        let mut buffer = StreamingBuffer::new(Duration::from_secs(60), 10);
        buffer.append("a", "hello", BufferMode::Accumulate);
        buffer.append("b", "world", BufferMode::Accumulate);
        let mut flushed = buffer.flush_all();
        flushed.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(
            flushed,
            vec![
                ("a".to_string(), "hello".to_string()),
                ("b".to_string(), "world".to_string())
            ]
        );
    }
}
