//! # codocia
//!
//! Skill owns capability metadata and turn-level activation planning.
//!
//! ## Owns
//! - skill catalog
//! - skill source metadata
//! - @skill mention parsing
//! - TurnPlan generation
//! - suggested tool activation
//!
//! ## Must Not
//! - render UI overlays
//! - write session history
//! - execute tools directly
//! - own durable Task or Run state
//!
//! ## Inputs
//! - user message text
//! - assigned skill IDs
//! - skill catalog
//!
//! ## Outputs
//! - TurnPlan
//! - activated skill IDs
//! - allowed tool names
//! - activation issues
//!
//! ## Depends On
//! - tool
//! - model
//!
//! ## Used By
//! - chat
//! - run
//!
//! ## Verify
//! - cargo check -p skill

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    System,
    User,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub source: Source,
    pub read_only: bool,
    pub suggested_tools: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    skills: BTreeMap<String, Skill>,
}

impl Catalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, skill: Skill) {
        self.skills.insert(skill.id.clone(), skill);
    }

    pub fn get(&self, id: &str) -> Option<&Skill> {
        self.skills.get(id)
    }

    pub fn list(&self) -> Vec<&Skill> {
        self.skills.values().collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnPlan {
    pub mentioned: Vec<String>,
    pub activated: Vec<String>,
    pub tools: Vec<String>,
    pub issues: Vec<String>,
}

pub fn mentions(input: &str) -> Vec<String> {
    input
        .split_whitespace()
        .filter_map(|part| part.strip_prefix('@'))
        .map(|id| {
            id.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_')
                .to_string()
        })
        .filter(|id| !id.is_empty())
        .collect()
}

pub fn plan_turn(catalog: &Catalog, assigned: &[String], input: &str) -> TurnPlan {
    let assigned: BTreeSet<&str> = assigned.iter().map(String::as_str).collect();
    let mut plan = TurnPlan {
        mentioned: mentions(input),
        tools: vec!["use_skill".to_string()],
        ..TurnPlan::default()
    };
    let mut tools: BTreeSet<String> = plan.tools.iter().cloned().collect();

    for id in &plan.mentioned {
        let Some(skill) = catalog.get(id) else {
            plan.issues.push(format!("unknown skill: {id}"));
            continue;
        };
        if !assigned.contains(id.as_str()) {
            plan.issues.push(format!("unassigned skill: {id}"));
            continue;
        }
        plan.activated.push(skill.id.clone());
        for tool in &skill.suggested_tools {
            if tools.insert(tool.clone()) {
                plan.tools.push(tool.clone());
            }
        }
    }

    plan
}
