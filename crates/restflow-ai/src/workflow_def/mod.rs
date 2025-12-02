//! WorkflowDef - Dynamic workflow definition
//!
//! This is the contract between Python and Rust:
//! - Python parses user code with `ast` module → WorkflowDef JSON
//! - Rust validates, compiles, and executes WorkflowDef

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single step execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepDef {
    /// Function name to call
    pub name: String,
    /// Output variable name
    pub output: String,
    /// Input variable references
    #[serde(default)]
    pub inputs: Vec<String>,
    /// Configuration for this step
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,
}

/// Parallel execution of multiple steps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelDef {
    /// Output variable names
    pub outputs: Vec<String>,
    /// Steps to execute in parallel
    pub steps: Vec<NodeDef>,
}

/// Conditional branching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionDef {
    /// Reference to registered predicate function
    pub predicate_id: String,
    /// Original predicate source for debugging
    pub predicate_source: String,
    /// Then branch
    pub then_branch: Box<NodeDef>,
    /// Else branch (optional)
    pub else_branch: Option<Box<NodeDef>>,
}

/// Loop construct
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoopDef {
    /// Iterator variable name
    pub iter_var: String,
    /// Iterable expression
    pub iterable: String,
    /// Loop body
    pub body: Box<NodeDef>,
}

/// Sequential execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceDef {
    /// Steps to execute in sequence
    pub steps: Vec<NodeDef>,
}

/// Node definition (tagged union)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NodeDef {
    Step(StepDef),
    Parallel(ParallelDef),
    Condition(ConditionDef),
    Loop(LoopDef),
    Sequence(SequenceDef),
}

/// Workflow parameter definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParameterDef {
    /// Parameter name
    pub name: String,
    /// Type hint (as string)
    pub type_hint: String,
    /// Default value (optional)
    pub default: Option<String>,
}

/// Complete workflow definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDef {
    /// Workflow name
    pub name: String,
    /// Input parameters
    pub parameters: Vec<ParameterDef>,
    /// Workflow body
    pub body: NodeDef,
    /// Return variable name
    pub return_var: Option<String>,
}

impl WorkflowDef {
    /// Parse WorkflowDef from JSON
    pub fn from_json(json: &str) -> anyhow::Result<Self> {
        Ok(serde_json::from_str(json)?)
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    /// Validate the workflow definition
    pub fn validate(&self) -> anyhow::Result<()> {
        // TODO: Implement validation
        // - Check all step names are valid
        // - Check data flow (variables defined before use)
        // - Check for cycles
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_def_serialization() {
        let workflow = WorkflowDef {
            name: "test_workflow".to_string(),
            parameters: vec![ParameterDef {
                name: "query".to_string(),
                type_hint: "str".to_string(),
                default: None,
            }],
            body: NodeDef::Sequence(SequenceDef {
                steps: vec![
                    NodeDef::Step(StepDef {
                        name: "search".to_string(),
                        output: "data".to_string(),
                        inputs: vec!["query".to_string()],
                        config: HashMap::new(),
                    }),
                    NodeDef::Step(StepDef {
                        name: "analyze".to_string(),
                        output: "result".to_string(),
                        inputs: vec!["data".to_string()],
                        config: HashMap::new(),
                    }),
                ],
            }),
            return_var: Some("result".to_string()),
        };

        let json = workflow.to_json().unwrap();
        let parsed = WorkflowDef::from_json(&json).unwrap();

        assert_eq!(parsed.name, "test_workflow");
        assert_eq!(parsed.parameters.len(), 1);
    }
}
