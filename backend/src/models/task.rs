use crate::models::{Node, Workflow};
use crate::engine::context::ExecutionContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowTask {
    pub id: String,
    pub execution_id: String,  
    pub node: Node,            
    pub workflow: Workflow,   
    pub context: ExecutionContext,  
    pub status: TaskStatus,
    pub created_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub input: Value,
    pub output: Option<Value>,
    pub error: Option<String>,
}

impl WorkflowTask {
    pub fn new(
        execution_id: String,
        node: Node,
        workflow: Workflow,
        context: ExecutionContext,
        input: Value,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            execution_id,
            node,
            workflow,
            context,
            status: TaskStatus::Pending,
            created_at: chrono::Utc::now().timestamp(),
            started_at: None,
            completed_at: None,
            input,
            output: None,
            error: None,
        }
    }

    pub fn new_single_node(node: Node, input: Value) -> Self {
        let workflow = Workflow {
            id: format!("single-{}", node.id),
            name: format!("Single Node: {}", node.id),
            nodes: vec![node.clone()],
            edges: vec![],
        };
        
        let execution_id = Uuid::new_v4().to_string();
        let context = ExecutionContext::new(execution_id.clone());
        
        Self::new(execution_id, node, workflow, context, input)
    }
}