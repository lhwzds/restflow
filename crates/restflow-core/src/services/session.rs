use crate::daemon::session_events::{ChatSessionEvent, publish_session_event};
use crate::models::{
    ChatMessage, ChatRole, ChatSession, ChatSessionSource, ChatSessionUpdate, MessageExecution,
    ModelId,
};
use crate::runtime::channel::hydrate_voice_message_metadata;
use crate::runtime::task_runtime::persist::persist_chat_session_memory;
use crate::services::session_policy::{
    SessionPolicy, SessionPolicyCleanupStats, SessionPolicyError,
};
use crate::session_log::{FileSession, FileSessionStore};
use crate::storage::{AgentStorage, MemoryStorage, SessionStorage, Storage, TaskStorage};
use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, Weak};
use tracing::{debug, warn};

#[derive(Clone)]
pub struct SessionService {
    sessions: SessionStorage,
    agents: Option<AgentStorage>,
    policy: SessionPolicy,
    memory: Option<MemoryStorage>,
    file_sessions: Option<FileSessionStore>,
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
        tasks: TaskStorage,
        memory: Option<MemoryStorage>,
    ) -> Self {
        let policy = SessionPolicy::new(sessions.clone(), tasks);
        Self {
            sessions,
            agents,
            policy,
            memory,
            file_sessions: None,
            append_locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn from_storage(storage: &Storage) -> Self {
        let service = Self::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        );
        service.with_default_file_sessions()
    }

    pub fn with_file_sessions(mut self, file_sessions: FileSessionStore) -> Self {
        self.file_sessions = Some(file_sessions);
        self
    }

    pub(crate) fn with_default_file_sessions(self) -> Self {
        #[cfg(not(test))]
        {
            let mut service = self;
            service.file_sessions = match FileSessionStore::open_default() {
                Ok(store) => Some(store),
                Err(error) => {
                    warn!(
                        error = %error,
                        "Failed to initialize file-backed session store"
                    );
                    None
                }
            };
            service
        }
        #[cfg(test)]
        {
            self
        }
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
        let (source, conversation_id) = self.effective_source(session)?;
        session.source_channel = Some(source);
        session.source_conversation_id = conversation_id;
        Ok(())
    }

    pub fn get_session_view(&self, session_id: &str) -> Result<Option<ChatSession>> {
        let mut session = if let Some(session) = self.sessions.get_session(session_id)? {
            session
        } else if let Some(store) = &self.file_sessions {
            match store.get(session_id)? {
                Some(session) => session.to_chat_session(),
                None => return Ok(None),
            }
        } else {
            return Ok(None);
        };
        session.hydrate_provider_from_model();
        self.apply_effective_source(&mut session)?;
        Ok(Some(session))
    }

    pub fn materialize_session_for_runtime(&self, session_id: &str) -> Result<Option<ChatSession>> {
        if let Some(mut session) = self.sessions.get_session(session_id)? {
            session.hydrate_provider_from_model();
            return Ok(Some(session));
        }

        let Some(mut session) = self.get_session_view(session_id)? else {
            return Ok(None);
        };
        session.hydrate_provider_from_model();
        self.sessions.save_session(&session)?;
        self.mirror_file_session(&session, "materialize_runtime");
        Ok(Some(session))
    }

    pub fn list_session_views(
        &self,
        agent_id: Option<&str>,
        skill_id: Option<&str>,
        include_archived: bool,
    ) -> Result<Vec<ChatSession>> {
        let mut sessions = match (agent_id, skill_id, include_archived) {
            (Some(agent_id), _, true) => self.sessions.chat_sessions.list_by_agent_all(agent_id)?,
            (Some(agent_id), _, false) => self.sessions.chat_sessions.list_by_agent(agent_id)?,
            (None, Some(skill_id), true) => {
                self.sessions.chat_sessions.list_by_skill_all(skill_id)?
            }
            (None, Some(skill_id), false) => self.sessions.chat_sessions.list_by_skill(skill_id)?,
            (None, None, true) => self.sessions.chat_sessions.list_all()?,
            (None, None, false) => self.sessions.chat_sessions.list()?,
        };

        for session in &mut sessions {
            session.hydrate_provider_from_model();
            self.apply_effective_source(session)?;
        }

        self.merge_file_session_views(&mut sessions, agent_id, skill_id, include_archived)?;
        sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));

        Ok(sessions)
    }

    pub fn search_session_views(
        &self,
        query: &str,
        agent_id: Option<&str>,
        skill_id: Option<&str>,
        include_archived: bool,
        limit: usize,
    ) -> Result<Vec<ChatSession>> {
        let keyword = query.to_lowercase();
        let sessions = self.list_session_views(agent_id, skill_id, include_archived)?;

        Ok(sessions
            .into_iter()
            .filter(|session| {
                session.name.to_lowercase().contains(&keyword)
                    || session
                        .messages
                        .iter()
                        .any(|message| message.content.to_lowercase().contains(&keyword))
            })
            .take(limit)
            .collect())
    }

    pub fn find_session_by_source_fields(
        &self,
        source_channel: ChatSessionSource,
        conversation_id: &str,
    ) -> Result<Option<ChatSession>> {
        for mut session in self.sessions.list_sessions_all()? {
            if session.source_channel == Some(source_channel)
                && session.source_conversation_id.as_deref() == Some(conversation_id)
            {
                session.hydrate_provider_from_model();
                return Ok(Some(session));
            }
        }

        let Some(store) = &self.file_sessions else {
            return Ok(None);
        };
        for file_session in store.list()? {
            let mut session = file_session.to_chat_session();
            if session.source_channel == Some(source_channel)
                && session.source_conversation_id.as_deref() == Some(conversation_id)
            {
                session.hydrate_provider_from_model();
                return Ok(Some(session));
            }
        }
        Ok(None)
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
        self.persist_session_view(&session, "create")?;
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
                .get_session_view(session_id)?
                .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

            session.hydrate_provider_from_model();
            session.add_message(user_message);
            session.add_message(assistant_message);

            if let Some(model) = final_model {
                session.set_model_identity(model);
            } else if let Some(model) = active_model {
                session.set_model_identity_from_raw(model);
            }

            self.persist_session_view(&session, "append_exchange")?;
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

    pub fn append_task_result(
        &self,
        session_id: &str,
        input: Option<&str>,
        output: &str,
        execution: MessageExecution,
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
                .get_session_view(session_id)?
                .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

            session.hydrate_provider_from_model();
            if let Some(input) = input
                && !input.trim().is_empty()
            {
                session.add_message(ChatMessage::user(input));
            }
            session.add_message(ChatMessage::assistant(output).with_execution(execution));
            self.persist_session_view(&session, "append_task_result")?;
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
                .get_session_view(session_id)?
                .ok_or_else(|| anyhow!("Session not found: {}", session_id))?;

            session.hydrate_provider_from_model();
            session.add_message(user_message);
            self.persist_session_view(&session, "append_user_message")?;
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
        let mut session = session.clone();
        session.hydrate_provider_from_model();
        self.persist_session_view(&session, "save")?;
        self.persist_memory(&session);
        publish_session_event(ChatSessionEvent::MessageAdded {
            session_id: session.id.clone(),
            source: source.to_string(),
        });
        Ok(())
    }

    pub fn create_external_session(&self, mut session: ChatSession) -> Result<ChatSession> {
        session.hydrate_provider_from_model();
        self.persist_session_view(&session, "create_external")?;
        publish_session_event(ChatSessionEvent::Created {
            session_id: session.id.clone(),
        });
        Ok(session)
    }

    pub fn save_session_metadata(&self, session: &ChatSession) -> Result<()> {
        let mut session = session.clone();
        session.hydrate_provider_from_model();
        self.persist_session_view(&session, "metadata")?;
        publish_session_event(ChatSessionEvent::Updated {
            session_id: session.id.clone(),
        });
        Ok(())
    }

    pub fn update_session(
        &self,
        session_id: &str,
        updates: ChatSessionUpdate,
    ) -> Result<Option<ChatSession>> {
        let Some(mut session) = self.get_session_view(session_id)? else {
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
            session.set_model_identity_from_raw(&normalized);
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
            self.persist_session_view(&session, "update")?;
            publish_session_event(ChatSessionEvent::Updated {
                session_id: session.id.clone(),
            });
        }

        self.apply_effective_source(&mut session)?;
        Ok(Some(session))
    }

    pub fn rename_session(&self, session_id: &str, name: String) -> Result<Option<ChatSession>> {
        let Some(mut session) = self.get_session_view(session_id)? else {
            return Ok(None);
        };
        self.policy
            .ensure_workspace_operation_allowed(&session, "renamed")?;
        session.rename(name);
        self.persist_session_view(&session, "rename")?;
        publish_session_event(ChatSessionEvent::Updated {
            session_id: session.id.clone(),
        });
        self.apply_effective_source(&mut session)?;
        Ok(Some(session))
    }

    pub fn switch_session_model(
        &self,
        session_id: &str,
        provider: String,
        model: String,
    ) -> Result<Option<ChatSession>> {
        let Some(mut session) = self.get_session_view(session_id)? else {
            return Ok(None);
        };
        self.policy
            .ensure_workspace_operation_allowed(&session, "switch model")?;
        session.provider = provider;
        session.model = model;
        session.updated_at = chrono::Utc::now().timestamp_millis();
        self.persist_session_view(&session, "switch_model")?;
        publish_session_event(ChatSessionEvent::Updated {
            session_id: session.id.clone(),
        });
        self.apply_effective_source(&mut session)?;
        Ok(Some(session))
    }

    pub fn archive_session(&self, session_id: &str) -> Result<bool> {
        let Some(mut session) = self.get_session_view(session_id)? else {
            return Ok(false);
        };
        self.policy
            .ensure_workspace_operation_allowed(&session, "archived")?;
        if session.is_archived() {
            return Ok(false);
        }
        session.archive();
        self.persist_session_view(&session, "archive")?;
        publish_session_event(ChatSessionEvent::Updated {
            session_id: session_id.to_string(),
        });
        Ok(true)
    }

    pub(crate) fn archive_managed_session(&self, session_id: &str) -> Result<bool> {
        let Some(mut session) = self.get_session_view(session_id)? else {
            return Ok(false);
        };
        if session.is_archived() {
            return Ok(false);
        }
        session.archive();
        self.persist_session_view(&session, "archive_managed")?;
        publish_session_event(ChatSessionEvent::Updated {
            session_id: session_id.to_string(),
        });
        Ok(true)
    }

    pub fn unarchive_session(&self, session_id: &str) -> Result<bool> {
        let Some(mut session) = self.get_session_view(session_id)? else {
            return Ok(false);
        };
        self.policy
            .ensure_workspace_operation_allowed(&session, "unarchived")?;
        if !session.is_archived() {
            return Ok(false);
        }
        session.unarchive();
        self.persist_session_view(&session, "unarchive")?;
        publish_session_event(ChatSessionEvent::Updated {
            session_id: session_id.to_string(),
        });
        Ok(true)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<bool> {
        let Some(session) = self.get_session_view(session_id)? else {
            return Ok(false);
        };
        self.policy
            .ensure_workspace_operation_allowed(&session, "deleted")?;

        let mut deleted = false;
        if self.sessions.get_session(session_id)?.is_some() {
            deleted = self.sessions.delete_session(session_id)?;
            if deleted {
                self.cleanup_session_artifacts(session_id)?;
            }
        }
        let file_deleted = self.delete_file_session(session_id);
        if deleted || file_deleted {
            publish_session_event(ChatSessionEvent::Deleted {
                session_id: session_id.to_string(),
            });
        }
        Ok(deleted || file_deleted)
    }

    pub fn rebuild_external_session(&self, session_id: &str) -> Result<Option<ChatSession>> {
        let Some(source_session) = self.sessions.get_session(session_id)? else {
            return Ok(None);
        };
        self.policy
            .ensure_external_rebuild_allowed(&source_session)?;

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
        self.mirror_file_session(&rebuilt, "rebuild_create");

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
            self.delete_file_session(&source_session.id);
            publish_session_event(ChatSessionEvent::Deleted {
                session_id: source_session.id.clone(),
            });
            publish_session_event(ChatSessionEvent::Created {
                session_id: rebuilt.id.clone(),
            });
            return Err(error);
        }

        self.apply_effective_source(&mut rebuilt)?;
        self.delete_file_session(&source_session.id);
        self.mirror_file_session(&rebuilt, "rebuild");
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
        if request.assistant_output.trim().is_empty() {
            anyhow::bail!("assistant_output must not be empty");
        }
        let _ = replace_latest_user_message_content(
            session,
            request.original_input,
            request.persisted_input,
        );
        session.hydrate_provider_from_model();
        session.add_message(
            ChatMessage::assistant(request.assistant_output).with_execution(request.execution),
        );
        if let Some(model) = request.final_model {
            session.set_model_identity(model);
        } else if let Some(model) = request.active_model {
            session.set_model_identity_from_raw(model);
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
        if matches!(
            source_channel,
            ChatSessionSource::Workspace | ChatSessionSource::Background
        ) {
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

    fn persist_session_view(&self, session: &ChatSession, operation: &'static str) -> Result<()> {
        let redb_exists = self.sessions.get_session(&session.id)?.is_some();
        let wrote_redb = redb_exists || self.file_sessions.is_none();
        if wrote_redb {
            self.sessions.save_session(session)?;
        }
        if let Err(error) = self.write_file_session(session) {
            if wrote_redb {
                warn!(
                    session_id = %session.id,
                    operation,
                    error = %error,
                    "Failed to mirror chat session to JSONL"
                );
            } else {
                return Err(error);
            }
        }
        Ok(())
    }

    fn merge_file_session_views(
        &self,
        sessions: &mut Vec<ChatSession>,
        agent_id: Option<&str>,
        skill_id: Option<&str>,
        include_archived: bool,
    ) -> Result<()> {
        let Some(store) = &self.file_sessions else {
            return Ok(());
        };
        let mut seen = sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<HashSet<_>>();
        for file_session in store.list()? {
            if !seen.insert(file_session.meta.id.clone()) {
                continue;
            }
            let mut session = file_session.to_chat_session();
            if !include_archived && session.archived_at.is_some() {
                continue;
            }
            if agent_id.is_some_and(|agent_id| session.agent_id != agent_id) {
                continue;
            }
            if skill_id.is_some_and(|skill_id| session.skill_id.as_deref() != Some(skill_id)) {
                continue;
            }
            session.hydrate_provider_from_model();
            self.apply_effective_source(&mut session)?;
            sessions.push(session);
        }
        Ok(())
    }

    fn mirror_file_session(&self, session: &ChatSession, operation: &'static str) {
        if let Err(error) = self.write_file_session(session) {
            warn!(
                session_id = %session.id,
                operation,
                error = %error,
                "Failed to mirror chat session to JSONL"
            );
        }
    }

    fn write_file_session(&self, session: &ChatSession) -> Result<()> {
        let Some(store) = &self.file_sessions else {
            return Ok(());
        };
        let existing = store.get(&session.id)?;
        let file_session = FileSession::merge_chat_session(existing.as_ref(), session);
        store.write_session(&file_session, true)?;
        Ok(())
    }

    fn delete_file_session(&self, session_id: &str) -> bool {
        let Some(store) = &self.file_sessions else {
            return false;
        };
        match store.delete(session_id) {
            Ok(deleted) => deleted,
            Err(error) => {
                warn!(
                    session_id,
                    error = %error,
                    "Failed to delete JSONL chat session"
                );
                false
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
        assert_eq!(persisted.provider, "openai");
        assert_eq!(persisted.model, "gpt-5");
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

        assert_eq!(persisted.provider, "minimax-coding-plan");
        assert_eq!(persisted.model, "minimax-coding-plan-m2-5");
    }

    #[test]
    fn append_task_result_persists_optional_input_and_execution() {
        let (storage, service, session) = setup();
        let execution = MessageExecution::new().complete(25, 7);

        let persisted = service
            .append_task_result(
                &session.id,
                Some("run digest"),
                "digest complete",
                execution.clone(),
                "task_runtime",
            )
            .unwrap();

        assert_eq!(persisted.messages.len(), 2);
        assert_eq!(persisted.messages[0].content, "run digest");
        assert_eq!(persisted.messages[1].content, "digest complete");
        assert_eq!(persisted.messages[1].execution.as_ref(), Some(&execution));
        let reloaded = storage.chat_sessions.get(&session.id).unwrap().unwrap();
        assert_eq!(reloaded.messages.len(), 2);
        assert_eq!(reloaded.messages[1].execution.as_ref(), Some(&execution));
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
    fn get_session_view_hydrates_provider_for_legacy_session() {
        let (storage, service, mut session) = setup();
        session.provider.clear();
        storage.chat_sessions.update(&session).unwrap();

        let hydrated = service
            .get_session_view(&session.id)
            .unwrap()
            .expect("session");

        assert_eq!(hydrated.provider, "openai");
        assert_eq!(hydrated.model, "gpt-5");
    }

    #[test]
    fn get_session_view_falls_back_to_file_session_store() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-service.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        file_store
            .write_session(&FileSession::from_chat_session(&session), false)
            .unwrap();
        let service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        )
        .with_file_sessions(file_store);

        let loaded = service
            .get_session_view(&session.id)
            .unwrap()
            .expect("file-backed session");

        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.agent_id, "agent-1");
    }

    #[test]
    fn get_session_view_propagates_invalid_file_session_errors() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-service.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let file_root = dir.path().join("sessions");
        let day_dir = file_root.join("2026").join("05").join("03");
        std::fs::create_dir_all(&day_dir).unwrap();
        std::fs::write(day_dir.join("broken-session.jsonl"), "{not-json}\n").unwrap();
        let service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        )
        .with_file_sessions(FileSessionStore::new(file_root).unwrap());

        let error = service
            .get_session_view("broken-session")
            .expect_err("invalid JSONL should be surfaced");

        assert!(error.to_string().contains("invalid JSONL"));
    }

    #[test]
    fn materialize_runtime_session_copies_file_session_to_redb() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-service.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::user("run this"));
        file_store
            .write_session(&FileSession::from_chat_session(&session), false)
            .unwrap();
        let service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        )
        .with_file_sessions(file_store);

        let materialized = service
            .materialize_session_for_runtime(&session.id)
            .unwrap()
            .expect("materialized");

        assert_eq!(materialized.id, session.id);
        assert!(storage.chat_sessions.get(&session.id).unwrap().is_some());
    }

    #[test]
    fn create_workspace_session_prefers_file_store_when_available() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-service.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        let service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        )
        .with_file_sessions(file_store.clone());

        let session = service
            .create_workspace_session(
                "agent-1".to_string(),
                "gpt-5".to_string(),
                Some("New imported path".to_string()),
                None,
                None,
            )
            .unwrap();

        assert!(storage.chat_sessions.get(&session.id).unwrap().is_none());
        assert!(file_store.get(&session.id).unwrap().is_some());
    }

    #[test]
    fn create_external_session_prefers_file_store_when_available() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-service.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        let service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        )
        .with_file_sessions(file_store.clone());
        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string())
            .with_source(ChatSessionSource::Telegram, "chat-1");

        let created = service.create_external_session(session).unwrap();
        let found = service
            .find_session_by_source_fields(ChatSessionSource::Telegram, "chat-1")
            .unwrap()
            .expect("file-backed external session");

        assert_eq!(found.id, created.id);
        assert_eq!(found.source_channel, Some(ChatSessionSource::Telegram));
        assert_eq!(found.source_conversation_id.as_deref(), Some("chat-1"));
        assert!(storage.chat_sessions.get(&created.id).unwrap().is_none());
        assert!(file_store.get(&created.id).unwrap().is_some());
    }

    #[test]
    fn rename_file_backed_session_without_materializing_redb_session() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-service.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        file_store
            .write_session(&FileSession::from_chat_session(&session), false)
            .unwrap();
        let service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        )
        .with_file_sessions(file_store.clone());

        let renamed = service
            .rename_session(&session.id, "Imported".to_string())
            .unwrap()
            .expect("renamed");

        assert_eq!(renamed.name, "Imported");
        assert!(storage.chat_sessions.get(&session.id).unwrap().is_none());
        assert_eq!(
            file_store
                .get(&session.id)
                .unwrap()
                .unwrap()
                .to_chat_session()
                .name,
            "Imported"
        );
    }

    #[test]
    fn delete_file_backed_session_without_redb_session() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-service.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        let session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        file_store
            .write_session(&FileSession::from_chat_session(&session), false)
            .unwrap();
        let service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        )
        .with_file_sessions(file_store.clone());

        assert!(service.delete_session(&session.id).unwrap());
        assert!(file_store.get(&session.id).unwrap().is_none());
        assert!(storage.chat_sessions.get(&session.id).unwrap().is_none());
    }

    #[test]
    fn save_existing_session_mirrors_to_file_session_store() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("session-service.db");
        let storage = Storage::new(db_path.to_str().unwrap()).unwrap();
        let file_store = FileSessionStore::new(dir.path().join("sessions")).unwrap();
        let service = SessionService::new(
            storage.sessions.clone(),
            Some(storage.agents.clone()),
            storage.tasks.clone(),
            Some(storage.memory.clone()),
        )
        .with_file_sessions(file_store.clone());
        let mut session = ChatSession::new("agent-1".to_string(), "gpt-5".to_string());
        session.add_message(ChatMessage::user("hello"));

        service.save_existing_session(&session, "test").unwrap();

        let loaded = file_store.get(&session.id).unwrap().expect("jsonl session");
        assert_eq!(loaded.to_chat_session().messages.len(), 1);
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
        storage
            .channel_session_bindings
            .upsert(&ChannelSessionBinding::new(
                "telegram",
                None,
                "conv-1",
                &session.id,
            ))
            .unwrap();

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
        assert_eq!(reloaded.provider, "openai");
        assert_eq!(reloaded.model, "gpt-5");
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
        assert_eq!(reloaded.provider, "minimax-coding-plan");
        assert_eq!(reloaded.model, "minimax-coding-plan-m2-5");
    }

    #[test]
    fn persist_interactive_turn_rejects_empty_assistant_output() {
        let (_storage, service, mut session) = setup();
        session.add_message(ChatMessage::user("voice input"));

        let error = service
            .persist_interactive_turn(
                &mut session,
                PersistInteractiveTurnRequest {
                    original_input: "voice input",
                    persisted_input: "voice transcript",
                    assistant_output: "   ",
                    active_model: Some("gpt-5"),
                    final_model: Some(ModelId::Gpt5),
                    execution: MessageExecution::new().complete(20, 1),
                    source: "ipc",
                },
            )
            .expect_err("empty assistant output should be rejected");

        assert!(
            error
                .to_string()
                .contains("assistant_output must not be empty")
        );
    }
}
