//! Graph module - Dynamic execution graph
//!
//! Unlike restflow-workflow's static DAG, this graph supports:
//! - Conditional branching (if/else)
//! - Loops (for/while)
//! - Agent loops (ReAct pattern)
//! - Runtime decisions

use serde::{Deserialize, Serialize};

/// Unique node identifier
pub type NodeId = String;

/// Unique graph identifier
pub type GraphId = String;

/// Reference to a function/step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionRef {
    /// Function name
    pub name: String,
    /// Module path (optional)
    pub module: Option<String>,
}

/// Input binding for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputBinding {
    /// Parameter name
    pub param: String,
    /// Source (variable name or expression)
    pub source: String,
}

/// Output binding for a node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputBinding {
    /// Output name
    pub name: String,
    /// Variable to store result
    pub target: String,
}

/// Step node - executes a single function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepNode {
    /// Node ID
    pub id: NodeId,
    /// Display name
    pub name: String,
    /// Function to call
    pub function_ref: FunctionRef,
    /// Input bindings
    pub inputs: Vec<InputBinding>,
    /// Output bindings
    pub outputs: Vec<OutputBinding>,
}

/// Loop kind
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LoopKind {
    /// for x in xs
    For,
    /// while condition
    While,
}

/// Expression (for conditions, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Expression {
    /// Expression source code
    pub source: String,
    /// Predicate ID for runtime evaluation
    pub predicate_id: Option<String>,
}

/// Tool reference for agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRef {
    /// Tool name
    pub name: String,
}

/// Graph node types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GraphNode {
    /// Simple step execution
    Step(StepNode),

    /// Conditional branching
    Condition {
        id: NodeId,
        condition: Expression,
        then_branch: Box<Graph>,
        else_branch: Option<Box<Graph>>,
    },

    /// Loop construct
    Loop {
        id: NodeId,
        kind: LoopKind,
        condition: Expression,
        body: Box<Graph>,
    },

    /// Agent loop (ReAct pattern)
    AgentLoop {
        id: NodeId,
        goal: String,
        tools: Vec<ToolRef>,
        max_iterations: usize,
        stop_condition: Option<Expression>,
    },

    /// Nested graph
    SubGraph {
        id: NodeId,
        graph: Box<Graph>,
    },

    /// Explicit parallel execution
    Parallel {
        id: NodeId,
        branches: Vec<Graph>,
    },
}

/// Edge between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source node
    pub from: NodeId,
    /// Target node
    pub to: NodeId,
    /// Optional condition
    pub condition: Option<Expression>,
}

/// Complete graph structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    /// Graph ID
    pub id: GraphId,
    /// Graph nodes
    pub nodes: Vec<GraphNode>,
    /// Edges between nodes
    pub edges: Vec<Edge>,
    /// Entry point nodes
    pub entry_points: Vec<NodeId>,
    /// Exit point nodes
    pub exit_points: Vec<NodeId>,
    /// Computed execution order
    #[serde(skip)]
    pub execution_order: Vec<NodeId>,
    /// Parallel groups (nodes that can run together)
    #[serde(skip)]
    pub parallel_groups: Vec<Vec<NodeId>>,
}

impl Graph {
    /// Create an empty graph
    pub fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            nodes: vec![],
            edges: vec![],
            entry_points: vec![],
            exit_points: vec![],
            execution_order: vec![],
            parallel_groups: vec![],
        }
    }

    /// Add a node to the graph
    pub fn add_node(&mut self, node: GraphNode) {
        self.nodes.push(node);
    }

    /// Add an edge between nodes
    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges.push(Edge {
            from: from.to_string(),
            to: to.to_string(),
            condition: None,
        });
    }

    /// Compute execution order and parallel groups
    pub fn analyze(&mut self) -> anyhow::Result<()> {
        // TODO: Implement topological sort and parallel group detection
        Ok(())
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphNode {
    /// Get the node ID
    pub fn id(&self) -> &NodeId {
        match self {
            GraphNode::Step(s) => &s.id,
            GraphNode::Condition { id, .. } => id,
            GraphNode::Loop { id, .. } => id,
            GraphNode::AgentLoop { id, .. } => id,
            GraphNode::SubGraph { id, .. } => id,
            GraphNode::Parallel { id, .. } => id,
        }
    }
}
