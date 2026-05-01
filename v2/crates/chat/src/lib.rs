//! # codocia
//!
//! Chat owns sessions, turns, and message history.
//!
//! ## Owns
//! - Session
//! - Message
//! - Role
//! - chat history composition
//! - session metadata required for resume and migration
//! - session repository helpers
//!
//! ## Must Not
//! - own durable background runs
//! - render TUI layout
//! - decide model catalog policy
//!
//! ## Inputs
//! - user messages
//! - assistant events
//! - tool events
//! - skill catalog
//! - session repository
//!
//! ## Outputs
//! - session history
//! - message lists
//! - agent run input
//! - persisted sessions
//!
//! ## Depends On
//! - agent
//! - event
//! - skill
//! - store
//!
//! ## Verify
//! - cargo check -p chat

use agent::RunInput;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use skill::{Catalog, resolve_context};
use store::Repository;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    Tool,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: Option<String>,
    pub agent_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub source: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived_at: Option<String>,
    pub messages: Vec<Message>,
}

impl Session {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: None,
            agent_id: None,
            provider: None,
            model: None,
            source: None,
            created_at: None,
            updated_at: None,
            archived_at: None,
            messages: Vec::new(),
        }
    }

    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
    }
}

pub async fn load_session<R>(repository: &R, id: &str) -> Result<Option<Session>>
where
    R: Repository<Session> + ?Sized,
{
    repository.get(id).await
}

pub async fn save_session<R>(repository: &R, session: Session) -> Result<()>
where
    R: Repository<Session> + ?Sized,
{
    repository.put(&session.id.clone(), session).await
}

pub async fn append_message<R>(
    repository: &R,
    session_id: &str,
    message: Message,
) -> Result<Session>
where
    R: Repository<Session> + ?Sized,
{
    let mut session = repository
        .get(session_id)
        .await?
        .unwrap_or_else(|| Session::new(session_id));
    session.push(message);
    repository.put(session_id, session.clone()).await?;
    Ok(session)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnRequest {
    pub message: String,
    pub assigned_skills: Vec<String>,
}

impl TurnRequest {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            assigned_skills: Vec::new(),
        }
    }

    pub fn with_assigned_skills(mut self, skills: impl IntoIterator<Item = String>) -> Self {
        self.assigned_skills = skills.into_iter().collect();
        self
    }

    pub fn to_agent_input(&self, catalog: &Catalog) -> RunInput {
        RunInput::new(self.message.clone()).with_skill_context(resolve_context(
            catalog,
            &self.assigned_skills,
            &self.message,
        ))
    }
}

pub fn build_agent_input(catalog: &Catalog, request: &TurnRequest) -> RunInput {
    request.to_agent_input(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;
    use skill::{Skill, Source};
    use std::future::Future;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    use store::MemoryStore;

    #[test]
    fn turn_request_resolves_skill_context_for_agent() {
        let mut catalog = Catalog::new();
        catalog.insert(
            Skill::new("team", "Team", Source::System)
                .with_description("Coordinate subagents.")
                .with_content("Use workers for independent tasks."),
        );
        let request =
            TurnRequest::new("please use @team").with_assigned_skills(["team".to_string()]);

        let input = build_agent_input(&catalog, &request);

        assert_eq!(input.message, "please use @team");
        assert_eq!(input.skill_context.assigned.len(), 1);
        assert_eq!(input.skill_context.mentioned.len(), 1);
        assert!(input.skill_context.issues.is_empty());
    }

    #[test]
    fn append_message_creates_and_updates_session() {
        block_on_once(async {
            let store = MemoryStore::new();

            let session = append_message(
                &store,
                "session-1",
                Message {
                    role: Role::User,
                    text: "hello".to_string(),
                },
            )
            .await
            .unwrap();

            assert_eq!(session.id, "session-1");
            assert_eq!(session.messages.len(), 1);

            let session = append_message(
                &store,
                "session-1",
                Message {
                    role: Role::Assistant,
                    text: "hi".to_string(),
                },
            )
            .await
            .unwrap();

            assert_eq!(session.messages.len(), 2);
            assert_eq!(
                load_session(&store, "session-1")
                    .await
                    .unwrap()
                    .unwrap()
                    .messages
                    .len(),
                2
            );
        });
    }

    #[test]
    fn save_session_replaces_existing_session() {
        block_on_once(async {
            let store = MemoryStore::new();
            let mut session = Session::new("session-1");
            session.push(Message {
                role: Role::System,
                text: "first".to_string(),
            });
            save_session(&store, session).await.unwrap();

            let mut replacement = Session::new("session-1");
            replacement.push(Message {
                role: Role::System,
                text: "replacement".to_string(),
            });
            save_session(&store, replacement).await.unwrap();

            let loaded = load_session(&store, "session-1").await.unwrap().unwrap();
            assert_eq!(loaded.messages.len(), 1);
            assert_eq!(loaded.messages[0].text, "replacement");
        });
    }

    fn block_on_once<T>(future: impl Future<Output = T>) -> T {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = std::pin::pin!(future);

        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("chat repository future unexpectedly yielded"),
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }
}
