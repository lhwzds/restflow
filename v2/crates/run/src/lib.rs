//! # codocia
//!
//! Run owns durable task and run execution concepts.
//!
//! ## Owns
//! - Task
//! - Run
//! - run status
//! - durable execution vocabulary
//!
//! ## Must Not
//! - become a second agent loop
//! - own skill catalog
//! - create a separate team runtime
//!
//! ## Inputs
//! - task definitions
//! - agent execution events
//! - checkpoint state
//!
//! ## Outputs
//! - run status
//! - run history
//! - run artifacts
//!
//! ## Depends On
//! - agent
//! - chat
//! - event
//! - skill
//! - store
//!
//! ## Verify
//! - cargo check -p run

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: String,
    pub task_id: String,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Pending,
    Running,
    Done,
    Failed,
    Canceled,
}
