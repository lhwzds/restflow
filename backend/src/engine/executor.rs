use crate::core::workflow::{Node, Workflow};
use crate::engine::context::ExecutionContext;
use crate::engine::graph::WorkflowGraph;
use crate::storage::{Storage, WorkflowTask};
use serde_json::Value;
use std::sync::Arc;
use tokio::task::JoinSet;
use uuid::Uuid;

pub struct WorkflowExecutor {
    workflow: Workflow,
    graph: WorkflowGraph,
    context: ExecutionContext,
    node_registry: Arc<crate::node::registry::NodeRegistry>,
}

impl WorkflowExecutor {
    pub fn new(workflow: Workflow) -> Self {
        Self {
            graph: WorkflowGraph::from_workflow(&workflow),
            context: ExecutionContext::new(workflow.id.clone()),
            workflow,
            node_registry: Arc::new(crate::node::registry::NodeRegistry::new()),
        }
    }

    pub async fn execute(&mut self) -> Result<Value, String> {
        let groups = self.graph.get_parallel_groups()?;
        
        for (stage, group) in groups.iter().enumerate() {
            self.log_stage_start(stage + 1, group);
            self.execute_parallel_group(group).await?;
        }

        self.build_result()
    }

    async fn execute_parallel_group(&mut self, group: &[String]) -> Result<(), String> {
        let mut tasks = JoinSet::new();
        
        for node_id in group {
            let node = self.get_node_checked(node_id)?;
            self.verify_dependencies(node_id)?;
            
            let context = self.context.clone();
            let registry = self.node_registry.clone();
            let node_clone = node.clone();
            
            tasks.spawn(async move {
                let mut ctx = context;
                let node_id = node_clone.id.clone();
                let result = Self::execute_node(&node_clone, &mut ctx, registry).await;
                (node_id, result, ctx)
            });
        }
        
        self.await_and_merge_results(tasks).await
    }

    async fn execute_node(
        node: &Node,
        context: &mut ExecutionContext,
        registry: Arc<crate::node::registry::NodeRegistry>,
    ) -> Result<Value, String> {
        println!("Executing node: {} (type: {:?})", node.id, node.node_type);
        
        let executor = registry.get(&node.node_type)
            .ok_or_else(|| format!("No executor found for node type: {:?}", node.node_type))?;
        
        let config = context.interpolate_value(&node.config);
        executor.execute(&config, context).await
    }

    // KISS: Wait for parallel nodes to complete and merge their results
    async fn await_and_merge_results(&mut self, mut tasks: JoinSet<(String, Result<Value, String>, ExecutionContext)>) -> Result<(), String> {
        while let Some(result) = tasks.join_next().await {
            let (node_id, execution_result, node_context) = result
                .map_err(|e| format!("Task join error: {}", e))?;
            
            match execution_result {
                Ok(output) => {
                    self.context.set_node_output(node_id.clone(), output);
                    self.merge_context(node_context);
                    println!("Node {} completed", node_id);
                }
                Err(err) => {
                    return Err(format!("Node {} failed: {}", node_id, err));
                }
            }
        }
        Ok(())
    }

    fn merge_context(&mut self, other: ExecutionContext) {
        for (key, value) in other.variables {
            self.context.set_variable(key, value);
        }
    }

    fn get_node_checked(&self, node_id: &str) -> Result<Node, String> {
        self.graph.get_node(node_id)
            .cloned()
            .ok_or_else(|| format!("Node {} not found", node_id))
    }

    fn verify_dependencies(&self, node_id: &str) -> Result<(), String> {
        for dep_id in self.graph.get_dependencies(node_id) {
            if !self.context.node_outputs.contains_key(&dep_id) {
                return Err(format!("Dependency {} not completed for node {}", dep_id, node_id));
            }
        }
        Ok(())
    }

    fn log_stage_start(&self, stage_num: usize, group: &[String]) {
        println!("Stage {}: executing {:?}", stage_num, group);
    }

    fn build_result(&self) -> Result<Value, String> {
        Ok(serde_json::json!({
            "execution_id": self.context.execution_id,
            "status": "completed",
            "results": self.context.node_outputs,
            "variables": self.context.variables
        }))
    }
}

pub struct AsyncWorkflowExecutor {
    storage: Arc<Storage>,
    running: Arc<tokio::sync::Mutex<bool>>,
    registry: Arc<crate::node::registry::NodeRegistry>,  // KISS: Reuse registry
}

impl AsyncWorkflowExecutor {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self {
            storage,
            running: Arc::new(tokio::sync::Mutex::new(false)),
            registry: Arc::new(crate::node::registry::NodeRegistry::new()),
        }
    }

    pub async fn start(&self) {
        if !self.try_start().await {
            return;
        }

        self.recover_stalled_tasks();
        self.spawn_worker().await;
    }

    async fn try_start(&self) -> bool {
        let mut running = self.running.lock().await;
        if *running {
            return false;
        }
        *running = true;
        true
    }

    fn recover_stalled_tasks(&self) {
        if let Err(e) = self.storage.queue.recover_stalled_tasks() {
            eprintln!("Failed to recover stalled tasks: {}", e);
        }
    }

    async fn spawn_worker(&self) {
        let storage = self.storage.clone();
        let running = self.running.clone();
        let registry = self.registry.clone();

        tokio::spawn(async move {
            println!("Async workflow executor started");
            
            while *running.lock().await {
                if let Err(e) = Self::process_next_task(&storage, &registry).await {
                    eprintln!("Error processing task: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
            
            println!("Async workflow executor stopped");
        });
    }

    async fn process_next_task(storage: &Arc<Storage>, registry: &Arc<crate::node::registry::NodeRegistry>) -> Result<(), String> {
        let task = storage.queue.pop().await
            .map_err(|e| format!("Failed to get task: {}", e))?;
        
        println!("Processing task: {}", task.id);
        
        let storage_clone = storage.clone();
        let registry_clone = registry.clone();
        tokio::spawn(async move {
            Self::execute_task(storage_clone, task, registry_clone).await;
        });
        
        Ok(())
    }

    async fn execute_task(storage: Arc<Storage>, task: WorkflowTask, registry: Arc<crate::node::registry::NodeRegistry>) {
        // All tasks are node tasks - execute and chain
        let result = Self::execute_and_queue_next(
            storage.clone(),
            task.node,
            task.workflow,
            task.execution_id,
            task.context,
            task.input,
            registry
        ).await;
        
        Self::update_task_status(storage, task.id, result).await;
    }

    // KISS: Execute node and queue downstream nodes that are ready
    async fn execute_and_queue_next(
        storage: Arc<Storage>,
        node: Node,
        workflow: Workflow,
        execution_id: String,
        mut context: ExecutionContext,
        _input: Value,
        registry: Arc<crate::node::registry::NodeRegistry>
    ) -> Result<Value, String> {
        // Execute the node
        let executor = registry.get(&node.node_type)
            .ok_or_else(|| format!("No executor for node type: {:?}", node.node_type))?;
        
        let config = context.interpolate_value(&node.config);
        let output = executor.execute(&config, &mut context).await?;
        
        // Update context with node output
        context.set_node_output(node.id.clone(), output.clone());
        
        // Find downstream nodes
        let graph = WorkflowGraph::from_workflow(&workflow);
        let downstream_nodes = graph.get_downstream_nodes(&node.id);
        
        // Queue downstream nodes if their dependencies are met
        for downstream_id in downstream_nodes {
            if let Some(downstream_node) = graph.get_node(&downstream_id) {
                // Check if all dependencies are satisfied
                let dependencies = graph.get_dependencies(&downstream_id);
                let all_deps_met = dependencies.iter().all(|dep| context.node_outputs.contains_key(dep));
                
                if all_deps_met {
                    storage.queue.push(
                        execution_id.clone(),
                        downstream_node.clone(),
                        workflow.clone(),
                        context.clone(),
                        Value::Null  // Downstream nodes get input from context
                    ).map_err(|e| format!("Failed to queue downstream node: {}", e))?;
                }
            }
        }
        
        Ok(output)
    }

    async fn update_task_status(storage: Arc<Storage>, task_id: String, result: Result<Value, String>) {
        match result {
            Ok(output) => {
                let _ = storage.queue.complete(&task_id, output);
                println!("Task {} completed successfully", task_id);
            }
            Err(error) => {
                let _ = storage.queue.fail(&task_id, error.clone());
                eprintln!("Task {} failed: {}", task_id, error);
            }
        }
    }

    pub async fn stop(&self) {
        *self.running.lock().await = false;
    }

    pub async fn submit(&self, workflow_id: String, input: Value) -> Result<String, String> {
        // Load workflow
        let workflow = self.storage.workflows.get_workflow(&workflow_id)
            .map_err(|e| format!("Failed to load workflow: {}", e))?
            .ok_or_else(|| format!("Workflow {} not found", workflow_id))?;
        
        // Decompose workflow into start node tasks
        let execution_id = Uuid::new_v4().to_string();
        let graph = WorkflowGraph::from_workflow(&workflow);
        let start_nodes = graph.get_nodes_with_no_dependencies();
        
        if start_nodes.is_empty() {
            return Err("No start nodes found in workflow".to_string());
        }
        
        // Create initial context
        let mut context = ExecutionContext::new(execution_id.clone());
        context.set_variable("input".to_string(), input.clone());
        
        // Push start nodes to queue
        for node_id in start_nodes {
            if let Some(node) = graph.get_node(&node_id) {
                self.storage.queue.push(
                    execution_id.clone(),
                    node.clone(),
                    workflow.clone(),
                    context.clone(),
                    input.clone()
                ).map_err(|e| format!("Failed to queue node: {}", e))?;
            }
        }
        
        Ok(execution_id)
    }
    
    pub async fn submit_node(&self, node: Node, input: Value) -> Result<String, String> {
        // Submit single node for execution
        self.storage.queue.push_single_node(node, input)
            .map_err(|e| format!("Failed to submit node: {}", e))
    }

    pub async fn get_task_status(&self, task_id: &str) -> Result<Option<WorkflowTask>, String> {
        self.storage.queue.get_task(task_id)
            .map_err(|e| format!("Failed to get task status: {}", e))
    }

    pub async fn list_tasks(
        &self,
        workflow_id: Option<&str>,
        status: Option<crate::storage::TaskStatus>,
    ) -> Result<Vec<WorkflowTask>, String> {
        self.storage.queue.list_tasks(workflow_id, status)
            .map_err(|e| format!("Failed to list tasks: {}", e))
    }
}