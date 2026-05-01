//! # codocia
//!
//! Engine session handling turns user chat input into agent events and history.
//!
//! ## Owns
//! - chat turn execution
//! - user message persistence
//! - assistant text finalization
//!
//! ## Must Not
//! - render UI
//! - decide transport streaming shape
//! - own durable storage backend details
//!
//! ## Inputs
//! - session id
//! - chat turn request
//!
//! ## Outputs
//! - agent run output
//! - appended chat messages
//!
//! ## Depends On
//! - agent
//! - chat
//! - event
//! - store
//!
//! ## Verify
//! - cargo test -p engine core_chat_turn_resolves_skill_and_persists_messages

use anyhow::Result;
use store::SharedStore;

use crate::Core;

impl Core {
    pub async fn chat_turn(
        &self,
        session_id: &str,
        request: chat::TurnRequest,
    ) -> Result<agent::RunOutput> {
        let catalog = self.skill_catalog().await?;
        let input = chat::build_agent_input(&catalog, &request);
        chat::append_message(
            &self.sessions,
            session_id,
            chat::Message {
                role: chat::Role::User,
                text: request.message,
            },
        )
        .await?;

        let output = agent::Exec::new(self.agent.clone(), self.tools.clone()).dry_run(input);
        persist_assistant_text(&self.sessions, session_id, &output.events).await?;
        Ok(output)
    }
}

async fn persist_assistant_text(
    sessions: &SharedStore<chat::Session>,
    session_id: &str,
    events: &[event::Event],
) -> Result<()> {
    let text = events
        .iter()
        .filter_map(|event| match event {
            event::Event::Text { value } => Some(value.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");

    if !text.is_empty() {
        chat::append_message(
            sessions,
            session_id,
            chat::Message {
                role: chat::Role::Assistant,
                text,
            },
        )
        .await?;
    }

    Ok(())
}
