use crate::daemon::session_events::{ChatSessionEvent, publish_session_event};
use crate::models::{AIModel, ChatMessage, ChatSession, ChatSessionSource};
use crate::runtime::background_agent::persist::persist_chat_session_memory;
use crate::services::session_lifecycle::{SessionLifecycleCleanupStats, SessionLifecycleService};
use crate::storage::{BackgroundAgentStorage, MemoryStorage, SessionStorage, Storage};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use tracing::{debug, warn};

#[derive(Clone)]
pub struct SessionService {
    sessions: SessionStorage,
    background_agents: BackgroundAgentStorage,
    memory: Option<MemoryStorage>,
    append_locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
}

impl SessionService {
    pub fn new(
        sessions: SessionStorage,
        background_agents: BackgroundAgentStorage,
        memory: Option<MemoryStorage>,
    ) -> Self {
        Self {
            sessions,
            background_agents,
            memory,
            append_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        Self::new(
            storage.sessions.clone(),
            storage.background_agents.clone(),
            Some(storage.memory.clone()),
        )
    }

    pub fn append_exchange(
        &self,
        session_id: &str,
        user_message: ChatMessage,
        assistant_message: ChatMessage,
        active_model: Option<&str>,
        source: &str,
    ) -> Result<ChatSession> {
        let session_lock = {
            let mut locks = self.append_locks.lock().expect("session append locks");
            if let Some(lock) = locks.get(session_id).and_then(Weak::upgrade) {
                lock
            } else {
                let lock = Arc::new(Mutex::new(()));
                locks.insert(session_id.to_string(), Arc::downgrade(&lock));
                lock
            }
        };

        let session = {
            let _guard = session_lock.lock().expect("session append lock");
            let mut session = self
                .sessions
                .chat_sessions
                .get(session_id)?
                .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

            session.add_message(user_message);
            session.add_message(assistant_message);

            if let Some(model) = active_model
                && let Some(normalized) = AIModel::normalize_model_id(model)
            {
                session.metadata.last_model = Some(normalized);
            }

            self.sessions.chat_sessions.save(&session)?;
            session
        };

        self.append_locks
            .lock()
            .expect("session append locks")
            .retain(|_, weak| weak.strong_count() > 0);

        self.persist_memory(&session);
        publish_session_event(ChatSessionEvent::MessageAdded {
            session_id: session_id.to_string(),
            source: source.to_string(),
        });

        Ok(session)
    }

    pub fn save_existing_session(&self, session: &ChatSession, source: &str) -> Result<()> {
        self.sessions.chat_sessions.update(session)?;
        self.persist_memory(session);
        publish_session_event(ChatSessionEvent::MessageAdded {
            session_id: session.id.clone(),
            source: source.to_string(),
        });
        Ok(())
    }

    pub fn management_owner(&self, session: &ChatSession) -> Result<Option<ChatSessionSource>> {
        self.lifecycle().management_owner(session)
    }

    pub fn is_workspace_managed(&self, session: &ChatSession) -> Result<bool> {
        self.lifecycle().is_workspace_managed(session)
    }

    pub fn archive_workspace_session(&self, session_id: &str) -> Result<bool> {
        self.lifecycle().archive_workspace_session(session_id)
    }

    pub fn delete_workspace_session(&self, session_id: &str) -> Result<bool> {
        self.lifecycle().delete_workspace_session(session_id)
    }

    pub fn cleanup_workspace_sessions_older_than(
        &self,
        older_than_ms: i64,
    ) -> Result<SessionLifecycleCleanupStats> {
        self.lifecycle()
            .cleanup_workspace_sessions_older_than(older_than_ms)
    }

    pub fn cleanup_workspace_sessions_by_retention(
        &self,
        now_ms: i64,
    ) -> Result<SessionLifecycleCleanupStats> {
        self.lifecycle()
            .cleanup_workspace_sessions_by_retention(now_ms)
    }

    pub fn cleanup_session_artifacts(&self, session_id: &str) -> Result<()> {
        self.lifecycle().cleanup_session_artifacts(session_id)
    }

    fn lifecycle(&self) -> SessionLifecycleService {
        SessionLifecycleService::new(self.sessions.clone(), self.background_agents.clone())
    }

    fn persist_memory(&self, session: &ChatSession) {
        let Some(memory) = &self.memory else {
            return;
        };
        match persist_chat_session_memory(memory, session) {
            Ok(Some(result)) if result.chunk_count > 0 => {
                debug!(
                    "Persisted {} memory chunks for chat session {}",
                    result.chunk_count, session.id
                );
            }
            Ok(Some(_)) | Ok(None) => {}
            Err(error) => {
                warn!(
                    session_id = %session.id,
                    error = %error,
                    "Failed to persist chat session memory"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChatMessage, ChatSessionSource, MessageExecution};
    use crate::storage::Storage;
    use tempfile::tempdir;

    fn setup() -> (Arc<Storage>, SessionService, ChatSession) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-service.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()).unwrap());
        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        storage.chat_sessions.create(&session).unwrap();
        let service = SessionService::from_storage(&storage);
        (storage, service, session)
    }

    #[test]
    fn append_exchange_persists_messages_and_model() {
        let (storage, service, session) = setup();
        let execution = MessageExecution::new().complete(10, 2);

        let persisted = service
            .append_exchange(
                &session.id,
                ChatMessage::user("hello"),
                ChatMessage::assistant("world").with_execution(execution),
                Some("gpt-5"),
                "channel",
            )
            .unwrap();

        assert_eq!(persisted.messages.len(), 2);
        assert_eq!(persisted.messages[0].content, "hello");
        assert_eq!(persisted.messages[1].content, "world");
        assert_eq!(persisted.metadata.last_model.as_deref(), Some("gpt-5"));
        let reloaded = storage.chat_sessions.get(&session.id).unwrap().unwrap();
        assert_eq!(reloaded.messages.len(), 2);
    }

    #[test]
    fn save_existing_session_updates_storage() {
        let (storage, service, mut session) = setup();
        session.add_message(ChatMessage::user("hello"));
        session.add_message(ChatMessage::assistant("world"));

        service.save_existing_session(&session, "ipc").unwrap();

        let reloaded = storage.chat_sessions.get(&session.id).unwrap().unwrap();
        assert_eq!(reloaded.messages.len(), 2);
        assert_eq!(reloaded.messages[0].content, "hello");
        assert_eq!(reloaded.messages[1].content, "world");
    }

    #[test]
    fn management_owner_delegates_lifecycle_rules() {
        let (storage, service, mut session) = setup();
        session.source_channel = Some(ChatSessionSource::Telegram);
        session.source_conversation_id = Some("conv-1".to_string());
        storage.chat_sessions.update(&session).unwrap();

        assert_eq!(
            service.management_owner(&session).unwrap(),
            Some(ChatSessionSource::Telegram)
        );
        assert!(!service.is_workspace_managed(&session).unwrap());
    }

    #[test]
    fn delete_workspace_session_delegates_lifecycle_rules() {
        let (storage, service, session) = setup();

        let deleted = service.delete_workspace_session(&session.id).unwrap();

        assert!(deleted);
        assert!(storage.chat_sessions.get(&session.id).unwrap().is_none());
    }
}
