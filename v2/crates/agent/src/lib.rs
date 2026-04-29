//! # codocia
//!
//! Agent owns the execution kernel and model/tool orchestration.
//!
//! ## Owns
//! - Agent
//! - execution input and output
//! - tool registry consumption
//! - event production
//!
//! ## Must Not
//! - own daemon lifecycle
//! - write durable storage directly
//! - render UI
//! - parse UI picker state
//!
//! ## Inputs
//! - Model
//! - allowed tools
//! - user message
//! - prompt context
//!
//! ## Outputs
//! - Event stream
//! - final run output
//!
//! ## Depends On
//! - event
//! - model
//! - tool
//!
//! ## Used By
//! - chat
//! - run
//!
//! ## Verify
//! - cargo check -p agent

use event::Event;
use model::Model;
use serde::{Deserialize, Serialize};
use tool::Registry;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub model: Model,
    pub skills: Vec<String>,
}

impl Agent {
    pub fn new(model: Model) -> Self {
        Self {
            model,
            skills: Vec::new(),
        }
    }

    pub fn with_skills(mut self, skills: impl IntoIterator<Item = String>) -> Self {
        self.skills = skills.into_iter().collect();
        self
    }
}

#[derive(Debug, Clone)]
pub struct RunInput {
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct RunOutput {
    pub events: Vec<Event>,
}

pub struct Exec {
    pub agent: Agent,
    pub tools: Registry,
}

impl Exec {
    pub fn new(agent: Agent, tools: Registry) -> Self {
        Self { agent, tools }
    }

    pub fn dry_run(&self, input: RunInput) -> RunOutput {
        RunOutput {
            events: vec![Event::Text {
                value: input.message,
            }],
        }
    }
}
