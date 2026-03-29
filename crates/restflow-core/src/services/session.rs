use crate::daemon::session_events::{ChatSessionEvent, publish_session_event};
use crate::models::{
    ChatMessage, ChatRole, ChatSession, ChatSessionSource, ChatSessionUpdate, MessageExecution,
    ModelId,
};
use crate::runtime::background_agent::persist::persist_chat_session_memory;
use crate::runtime::channel::hydrate_voice_message_metadata;
use crate::services::session_policy::{
    SessionPolicy, SessionPolicyCleanupStats, SessionPolicyError,
};
use crate::storage::{
    AgentStorage, BackgroundAgentStorage, MemoryStorage, SessionStorage, Storage,
};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use tracing::{debug, warn};

#[derive(Clone)]
pub struct SessionService {
    sessions: SessionStorage,
    agents: Option<AgentStorage>,
    policy: SessionPolicy,
    memory: Option<MemoryStorage>,
    append_locks: Arc<Mutex<HashMap<String, Weak<Mutex<()>>>>>,
}

pub struct PersistInteractiveTurnRequest<'a> {
    pub original_input: &'a str,
    pub persisted_input: &'a str,
    pub assistant_output: &'a str,
    pub active_model: Option<&'a str>,
    pub final_model: Option<ModelId>,
    pub execution: MessageExecution,
    pub source: &'a str,
}

impl SessionService {
    pub fn new(
        sessions: SessionStorage,
        agents: Option<AgentStorage>,
        background_agents: BackgroundAgentStorage,
        memory: Option<MemoryStorage>,
    ) -> Self {
        let policy = SessionPolicy::new(sessions.clone(), background_agents);
        Self {
            sessions,
            agents,
            policy,
            memory,
            append_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        Self::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.background_agents.clone(),
            Some(storage.memory.clone()),
        )
    }

    pub fn management_owner(&self, session: &ChatSession) -> Result<Option<ChatSessionSource>> {
        self.policy.management_owner(session)
    }

    pub fn effective_source(
        &self,
        session: &ChatSession,
    ) -> Result<(ChatSessionSource, Option<String>)> {
        let effective = self.policy.effective_source(session)?;
        Ok((effective.source, effective.conversation_id))
    }

    pub fn apply_effective_source(&self, session: &mut ChatSession) -> Result<()> {
        if self
            .sessions
            .list_bindings_by_session(&session.id)?
            .is_empty()
        {
            let _ = self.sessions.ensure_binding_from_legacy_source(session)?;
        }
        let (source, conversation_id) = self.effective_source(session)?;
        session.source_channel = Some(source);
        session.source_conversation_id = conversation_id;
        Ok(())
    }

    pub fn get_session_view(&self, session_id: &str) -> Result<Option<ChatSession>> {
        let Some(mut session) = self.sessions.get_session(session_id)? else {
            return Ok(None);
        };
        self.apply_effective_source(&mut session)?;
        Ok(Some(session))
    }

    pub fn create_workspace_session(
        &self,
        agent_id: String,
        model: String,
        name: Option<String>,
        skill_id: Option<String>,
        retention: Option<String>,
    ) -> Result<ChatSession> {
        let mut session = ChatSession::new(agent_id, model);
        session.source_channel = Some(ChatSessionSource::Workspace);
        if let Some(name) = name {
            session = session.with_name(name);
        }
        if let Some(skill_id) = skill_id {
            session = session.with_skill(skill_id);
        }
        if let Some(retention) = retention {
            session = session.with_retention(retention);
        }
        self.sessions.create_session(&session)?;
        self.apply_effective_source(&mut session)?;
        publish_session_event(ChatSessionEvent::Created {
            session_id: session.id.clone(),
        });
        Ok(session)
    }

    pub fn is_workspace_managed(&self, session: &ChatSession) -> Result<bool> {
        self.policy.is_workspace_managed(session)
    }

    pub fn append_exchange(
        &self,
        session_id: &str,
        user_message: ChatMessage,
        assistant_message: ChatMessage,
        active_model: Option<&str>,
        final_model: Option<ModelId>,
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
                .get_session(session_id)?
                .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

            session.add_message(user_message);
            session.add_message(assistant_message);

            if let Some(model) = final_model {
                session.metadata.last_model = Some(model.as_serialized_str().to_string());
            } else if let Some(model) = active_model
                && let Some(normalized) = ModelId::normalize_model_id(model)
            {
                session.metadata.last_model = Some(normalized);
            }

            self.sessions.save_session(&session)?;
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

    pub fn append_user_message(
        &self,
        session_id: &str,
        mut user_message: ChatMessage,
        source: &str,
    ) -> Result<ChatSession> {
        hydrate_voice_message_metadata(&mut user_message);

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
                .get_session(session_id)?
                .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

            session.add_message(user_message);
            self.sessions.save_session(&session)?;
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
        self.sessions.update_session(session)?;
        self.persist_memory(session);
        publish_session_event(ChatSessionEvent::MessageAdded {
            session_id: session.id.clone(),
            source: source.to_string(),
        });
        Ok(())
    }

    pub fn update_session(
        &self,
        session_id: &str,
        updates: ChatSessionUpdate,
    ) -> Result<Option<ChatSession>> {
        let Some(mut session) = self.sessions.get_session(session_id)? else {
            return Ok(None);
        };
        self.policy
            .ensure_workspace_operation_allowed(&session, "updated")?;

        let mut updated = false;
        let mut name_updated = false;

        if let Some(agent_id) = updates.agent_id {
            let agents = self
                .agents
                .as_ref()
                .ok_or_else(|| anyhow!("Agent storage is unavailable"))?;
            session.agent_id = agents.resolve_existing_agent_id(&agent_id)?;
            updated = true;
        }

        if let Some(model) = updates.model {
            let normalized = ModelId::normalize_model_id(&model)
                .ok_or_else(|| anyhow!("Unknown model: {}", model.trim()))?;
            session.model = normalized;
            updated = true;
        }

        if let Some(name) = updates.name {
            session.rename(name);
            updated = true;
            name_updated = true;
        }

        if updated {
            if !name_updated {
                session.updated_at = chrono::Utc::now().timestamp_millis();
            }
            self.sessions.update_session(&session)?;
            publish_session_event(ChatSessionEvent::Updated {
                session_id: session.id.clone(),
            });
        }

        self.apply_effective_source(&mut session)?;
        Ok(Some(session))
    }

    pub fn rename_session(&self, session_id: &str, name: String) -> Result<Option<ChatSession>> {
        let Some(mut session) = self.sessions.get_session(session_id)? else {
            return Ok(None);
        };
        self.policy
            .ensure_workspace_operation_allowed(&session, "renamed")?;
        session.rename(name);
        self.sessions.update_session(&session)?;
        publish_session_event(ChatSessionEvent::Updated {
            session_id: session.id.clone(),
        });
        self.apply_effective_source(&mut session)?;
        Ok(Some(session))
    }

    pub fn archive_session(&self, session_id: &str) -> Result<bool> {
        let archived = self.policy.archive_workspace_session(session_id)?;
        if archived {
            publish_session_event(ChatSessionEvent::Updated {
                session_id: session_id.to_string(),
            });
        }
        Ok(archived)
    }

    pub fn unarchive_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self.sessions.get_session(session_id)? else {
            return Ok(false);
        };
        self.policy
            .ensure_workspace_operation_allowed(&session, "unarchived")?;
        let unarchived = self.sessions.unarchive_session(session_id)?;
        if unarchived {
            publish_session_event(ChatSessionEvent::Updated {
                session_id: session_id.to_string(),
            });
        }
        Ok(unarchived)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        let deleted = self.policy.delete_workspace_session(session_id)?;
        if deleted {
            publish_session_event(ChatSessionEvent::Deleted {
                session_id: session_id.to_string(),
            });
        }
        Ok(deleted)
    }

    pub fn rebuild_external_session(&self, session_id: &str) -> Result<Option<ChatSession>> {
        let Some(source_session) = self.sessions.get_session(session_id)? else {
            return Ok(None);
        };
        self.policy
            .ensure_external_rebuild_allowed(&source_session)?;

        let _ = self
            .sessions
            .ensure_binding_from_legacy_source(&source_session)?;
        let (source_channel, conversation_id) = self.effective_source(&source_session)?;
        let conversation_id = conversation_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                anyhow!(SessionPolicyError::MissingExternalRoute {
                    session_id: source_session.id.clone(),
                    operation: "rebuilt",
                })
            })?;

        let mut rebuilt =
            self.build_rebuilt_external_session(&source_session, source_channel, conversation_id)?;
        self.sessions.create_session(&rebuilt)?;

        let switched_bindings = match self
            .sessions
            .switch_bindings(&source_session.id, &rebuilt.id)
        {
            Ok(bindings) => bindings,
            Err(error) => {
                let _ = self.sessions.delete_session(&rebuilt.id);
                return Err(error);
            }
        };

        match self.sessions.delete_session(&source_session.id) {
            Ok(true) => {}
            Ok(false) => {
                let _ = self.restore_bindings(&switched_bindings);
                let _ = self.sessions.delete_session(&rebuilt.id);
                return Err(anyhow!(
                    "Failed to delete original session {} during rebuild",
                    source_session.id
                ));
            }
            Err(error) => {
                let _ = self.restore_bindings(&switched_bindings);
                let _ = self.sessions.delete_session(&rebuilt.id);
                return Err(error);
            }
        }

        if let Err(error) = self.sessions.cleanup_artifacts(&source_session.id) {
            publish_session_event(ChatSessionEvent::Deleted {
                session_id: source_session.id.clone(),
            });
            publish_session_event(ChatSessionEvent::Created {
                session_id: rebuilt.id.clone(),
            });
            return Err(error);
        }

        self.apply_effective_source(&mut rebuilt)?;
        publish_session_event(ChatSessionEvent::Deleted {
            session_id: source_session.id.clone(),
        });
        publish_session_event(ChatSessionEvent::Created {
            session_id: rebuilt.id.clone(),
        });
        Ok(Some(rebuilt))
    }

    pub fn cleanup_session_artifacts(&self, session_id: &str) -> Result<()> {
        self.policy.cleanup_session_artifacts(session_id)
    }

    pub fn cleanup_workspace_sessions_older_than(
        &self,
        older_than_ms: i64,
    ) -> Result<SessionPolicyCleanupStats> {
        self.policy
            .cleanup_workspace_sessions_older_than(older_than_ms)
    }

    pub fn cleanup_workspace_sessions_by_retention(
        &self,
        now_ms: i64,
    ) -> Result<SessionPolicyCleanupStats> {
        self.policy.cleanup_workspace_sessions_by_retention(now_ms)
    }

    pub fn persist_interactive_turn(
        &self,
        session: &mut ChatSession,
        request: PersistInteractiveTurnRequest<'_>,
    ) -> Result<()> {
        let _ = replace_latest_user_message_content(
            session,
            request.original_input,
            request.persisted_input,
        );
        session.add_message(
            ChatMessage::assistant(request.assistant_output).with_execution(request.execution),
        );
        if let Some(model) = request.final_model {
            session.metadata.last_model = Some(model.as_serialized_str().to_string());
        } else if let Some(model) = request.active_model
            && let Some(normalized) = ModelId::normalize_model_id(model)
        {
            session.metadata.last_model = Some(normalized);
        }
        self.save_existing_session(session, request.source)
    }

    pub fn archive_workspace_session(&self, session_id: &str) -> Result<bool> {
        self.archive_session(session_id)
    }

    pub fn unarchive_workspace_session(&self, session_id: &str) -> Result<bool> {
        self.unarchive_session(session_id)
    }

    pub fn delete_workspace_session(&self, session_id: &str) -> Result<bool> {
        self.delete_session(session_id)
    }

    fn build_rebuilt_external_session(
        &self,
        source: &ChatSession,
        source_channel: ChatSessionSource,
        conversation_id: &str,
    ) -> Result<ChatSession> {
        let conversation_id = conversation_id.trim();
        if conversation_id.is_empty() {
            anyhow::bail!("External session is missing conversation_id");
        }
        if source_channel == ChatSessionSource::Workspace {
            anyhow::bail!("Session is not externally managed");
        }

        let mut rebuilt = ChatSession::new(source.agent_id.clone(), source.model.clone())
            .with_name(format!("channel:{}", conversation_id))
            .with_source(source_channel, conversation_id.to_string());

        if let Some(skill_id) = source.skill_id.clone() {
            rebuilt = rebuilt.with_skill(skill_id);
        }
        if let Some(retention) = source.retention.clone() {
            rebuilt = rebuilt.with_retention(retention);
        }

        rebuilt.source_channel = Some(source_channel);
        rebuilt.source_conversation_id = Some(conversation_id.to_string());
        Ok(rebuilt)
    }

    fn restore_bindings(&self, bindings: &[crate::models::ChannelSessionBinding]) -> Result<()> {
        for binding in bindings {
            self.sessions.upsert_binding(binding)?;
        }
        Ok(())
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

fn replace_latest_user_message_content(
    session: &mut ChatSession,
    original_content: &str,
    updated_content: &str,
) -> bool {
    if original_content == updated_content {
        return false;
    }

    let Some(index) = session
        .messages
        .iter()
        .rposition(|message| message.role == ChatRole::User && message.content == original_content)
    else {
        return false;
    };

    session.messages[index].content = updated_content.to_string();
    hydrate_voice_message_metadata(&mut session.messages[index]);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChannelSessionBinding, MessageExecution};
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
                Some(ModelId::Gpt5),
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
    fn append_exchange_prefers_provider_aware_final_model() {
        let (_storage, service, session) = setup();

        let persisted = service
            .append_exchange(
                &session.id,
                ChatMessage::user("hello"),
                ChatMessage::assistant("world"),
                Some("MiniMax-M2.5"),
                Some(ModelId::MiniMaxM25CodingPlan),
                "channel",
            )
            .unwrap();

        assert_eq!(
            persisted.metadata.last_model.as_deref(),
            Some("minimax-coding-plan-m2-5")
        );
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
    fn append_user_message_hydrates_voice_metadata() {
        let (storage, service, session) = setup();

        let persisted = service
            .append_user_message(
                &session.id,
                ChatMessage::user(
                    "[Voice message]\n\n[Media Context]\nmedia_type: voice\nlocal_file_path: /tmp/voice.webm\n\n[Transcript]\nhello voice",
                ),
                "ipc",
            )
            .unwrap();

        assert_eq!(persisted.messages.len(), 1);
        let user = &persisted.messages[0];
        assert_eq!(user.role, ChatRole::User);
        assert_eq!(
            user.media.as_ref().map(|media| media.file_path.as_str()),
            Some("/tmp/voice.webm")
        );
        assert_eq!(
            user.transcript
                .as_ref()
                .map(|transcript| transcript.text.as_str()),
            Some("hello voice")
        );

        let reloaded = storage.chat_sessions.get(&session.id).unwrap().unwrap();
        assert_eq!(reloaded.messages.len(), 1);
    }

    #[test]
    fn management_owner_delegates_policy_rules() {
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
    fn update_session_enforces_workspace_policy_and_persists_changes() {
        let (storage, service, session) = setup();
        let updated = service
            .update_session(
                &session.id,
                ChatSessionUpdate {
                    agent_id: None,
                    model: Some("gpt-5".to_string()),
                    name: Some("Updated".to_string()),
                },
            )
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "Updated");
        let reloaded = storage.chat_sessions.get(&session.id).unwrap().unwrap();
        assert_eq!(reloaded.name, "Updated");
    }

    #[test]
    fn apply_effective_source_backfills_legacy_binding() {
        let (_storage, service, mut session) = setup();
        session.source_channel = Some(ChatSessionSource::Telegram);
        session.source_conversation_id = Some("chat-1".to_string());

        service.apply_effective_source(&mut session).unwrap();

        assert_eq!(session.source_channel, Some(ChatSessionSource::Telegram));
        assert_eq!(session.source_conversation_id.as_deref(), Some("chat-1"));
    }

    #[test]
    fn rebuild_external_session_switches_binding_and_deletes_old_session() {
        let (storage, service, mut session) = setup();
        session.source_channel = Some(ChatSessionSource::Telegram);
        session.source_conversation_id = Some("chat-1".to_string());
        storage.chat_sessions.update(&session).unwrap();
        storage
            .channel_session_bindings
            .upsert(&ChannelSessionBinding::new(
                "telegram",
                None,
                "chat-1",
                &session.id,
            ))
            .unwrap();

        let rebuilt = service
            .rebuild_external_session(&session.id)
            .unwrap()
            .expect("rebuilt session");

        assert_ne!(rebuilt.id, session.id);
        assert!(storage.chat_sessions.get(&session.id).unwrap().is_none());
        let binding = storage
            .channel_session_bindings
            .get_by_route("telegram", None, "chat-1")
            .unwrap()
            .unwrap();
        assert_eq!(binding.session_id, rebuilt.id);
    }

    #[test]
    fn persist_interactive_turn_rewrites_latest_input_and_appends_output() {
        let (storage, service, mut session) = setup();
        session.add_message(ChatMessage::user("voice input"));
        storage.chat_sessions.update(&session).unwrap();

        service
            .persist_interactive_turn(
                &mut session,
                PersistInteractiveTurnRequest {
                    original_input: "voice input",
                    persisted_input: "voice transcript",
                    assistant_output: "assistant output",
                    active_model: Some("gpt-5"),
                    final_model: Some(ModelId::Gpt5),
                    execution: MessageExecution::new().complete(20, 1),
                    source: "ipc",
                },
            )
            .unwrap();

        let reloaded = storage.chat_sessions.get(&session.id).unwrap().unwrap();
        assert_eq!(reloaded.messages.len(), 2);
        assert_eq!(reloaded.messages[0].content, "voice transcript");
        assert_eq!(reloaded.messages[1].content, "assistant output");
        assert_eq!(reloaded.metadata.last_model.as_deref(), Some("gpt-5"));
    }

    #[test]
    fn persist_interactive_turn_prefers_provider_aware_final_model() {
        let (storage, service, mut session) = setup();
        session.add_message(ChatMessage::user("voice input"));
        storage.chat_sessions.update(&session).unwrap();

        service
            .persist_interactive_turn(
                &mut session,
                PersistInteractiveTurnRequest {
                    original_input: "voice input",
                    persisted_input: "voice transcript",
                    assistant_output: "assistant output",
                    active_model: Some("MiniMax-M2.5"),
                    final_model: Some(ModelId::MiniMaxM25CodingPlan),
                    execution: MessageExecution::new().complete(20, 1),
                    source: "ipc",
                },
            )
            .unwrap();

        let reloaded = storage.chat_sessions.get(&session.id).unwrap().unwrap();
        assert_eq!(
            reloaded.metadata.last_model.as_deref(),
            Some("minimax-coding-plan-m2-5")
        );
    }
}
