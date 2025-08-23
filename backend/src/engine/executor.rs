use crate::models::{Node, Workflow};
use crate::engine::context::ExecutionContext;
use crate::engine::graph::WorkflowGraph;
use crate::engine::scheduler::Scheduler;
use crate::models::WorkflowTask;
use crate::storage::Storage;
use serde_json::Value;
use std::sync::Arc;
use tokio::task::JoinSet;
use tokio::sync::Mutex;
use uuid::Uuid;

const QUEUE_POLL_INTERVAL_MS: u64 = 100;

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
    scheduler: Arc<Scheduler>,
    running: Arc<tokio::sync::Mutex<bool>>,
    registry: Arc<crate::node::registry::NodeRegistry>,  // KISS: Reuse registry
    num_workers: usize,  // Number of concurrent workers
}

impl AsyncWorkflowExecutor {
    pub fn new(storage: Arc<Storage>) -> Self {
        Self::with_workers(storage, 1)  // Default to 1 worker
    }
    
    pub fn with_workers(storage: Arc<Storage>, num_workers: usize) -> Self {
        let scheduler = Arc::new(Scheduler::new(storage.queue.clone()));
        Self {
            storage,
            scheduler,
            running: Arc::new(tokio::sync::Mutex::new(false)),
            registry: Arc::new(crate::node::registry::NodeRegistry::new()),
            num_workers: num_workers.max(1),  // At least 1 worker
        }
    }

    pub async fn start(&self) {
        if !self.try_start().await {
            return;
        }

        self.recover_stalled_tasks();
        self.spawn_workers().await;
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
        if let Err(e) = self.scheduler.recover_stalled_tasks() {
            eprintln!("Failed to recover stalled tasks: {}", e);
        }
    }

    async fn spawn_workers(&self) {
        println!("Starting {} workers", self.num_workers);
        
        for worker_id in 0..self.num_workers {
            let worker = Worker::new(
                worker_id,
                self.storage.clone(),
                self.scheduler.clone(),
                self.registry.clone(),
                self.running.clone()
            );
            
            tokio::spawn(async move {
                worker.run().await;
            });
        }
    }

    async fn execute_task(_storage: Arc<Storage>, scheduler: Arc<Scheduler>, task: WorkflowTask, registry: Arc<crate::node::registry::NodeRegistry>) {
        // Execute the node
        let result = Self::execute_node(&task.node, &task.context, &registry).await;
        
        // Handle the result and queue downstream tasks if successful
        match result {
            Ok(output) => {
                let chain_result = scheduler.queue_downstream_tasks(
                    &task,
                    output.clone()
                );
                
                // Report success even if chaining fails (task itself succeeded)
                if let Err(e) = chain_result {
                    eprintln!("Failed to queue downstream tasks: {}", e);
                }
                Self::update_task_status(scheduler, task.id.clone(), Ok(output)).await;
            }
            Err(e) => {
                Self::update_task_status(scheduler, task.id.clone(), Err(e)).await;
            }
        }
    }

    // KISS: Pure node execution logic
    async fn execute_node(
        node: &Node,
        context: &ExecutionContext,
        registry: &Arc<crate::node::registry::NodeRegistry>
    ) -> Result<Value, String> {
        let executor = registry.get(&node.node_type)
            .ok_or_else(|| format!("No executor for node type: {:?}", node.node_type))?;
        
        let mut ctx = context.clone();
        let config = ctx.interpolate_value(&node.config);
        let output = executor.execute(&config, &mut ctx).await?;
        
        Ok(output)
    }


    async fn update_task_status(scheduler: Arc<Scheduler>, task_id: String, result: Result<Value, String>) {
        match result {
            Ok(output) => {
                let _ = scheduler.complete_task(&task_id, output);
                println!("Task {} completed successfully", task_id);
            }
            Err(error) => {
                let _ = scheduler.fail_task(&task_id, error.clone());
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
                self.scheduler.push_task(
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
        self.scheduler.push_single_node(node, input)
            .map_err(|e| format!("Failed to submit node: {}", e))
    }

    pub async fn get_task_status(&self, task_id: &str) -> Result<Option<WorkflowTask>, String> {
        self.scheduler.get_task(task_id)
            .map_err(|e| format!("Failed to get task status: {}", e))
    }
    
    pub async fn get_execution_status(&self, execution_id: &str) -> Result<Vec<WorkflowTask>, String> {
        self.scheduler.get_tasks_by_execution(execution_id)
            .map_err(|e| format!("Failed to get execution status: {}", e))
    }

    pub async fn list_tasks(
        &self,
        workflow_id: Option<&str>,
        status: Option<crate::models::TaskStatus>,
    ) -> Result<Vec<WorkflowTask>, String> {
        self.scheduler.list_tasks(workflow_id, status)
            .map_err(|e| format!("Failed to list tasks: {}", e))
    }
}

struct Worker {
    id: usize,
    storage: Arc<Storage>,
    scheduler: Arc<Scheduler>,
    registry: Arc<crate::node::registry::NodeRegistry>,
    running: Arc<Mutex<bool>>,
}

impl Worker {
    fn new(
        id: usize,
        storage: Arc<Storage>,
        scheduler: Arc<Scheduler>,
        registry: Arc<crate::node::registry::NodeRegistry>,
        running: Arc<Mutex<bool>>
    ) -> Self {
        Self { id, storage, scheduler, registry, running }
    }
    
    async fn run(&self) {
        println!("Worker {} started", self.id);
        
        while *self.running.lock().await {
            if let Err(e) = self.process_next_task().await {
                // Only log error if it's not a queue empty error
                if !e.contains("Failed to get task") {
                    eprintln!("Worker {} error: {}", self.id, e);
                }
                // Brief sleep to avoid busy waiting when queue is empty
                tokio::time::sleep(tokio::time::Duration::from_millis(QUEUE_POLL_INTERVAL_MS)).await;
            }
        }
        
        println!("Worker {} stopped", self.id);
    }
    
    async fn process_next_task(&self) -> Result<(), String> {
        let task = self.scheduler.pop_task().await
            .map_err(|e| format!("Failed to get task: {}", e))?;
        
        println!("Worker {} processing task: {} (node: {})", self.id, task.id, task.node.id);
        
        AsyncWorkflowExecutor::execute_task(self.storage.clone(), self.scheduler.clone(), task, self.registry.clone()).await;
        
        Ok(())
    }
}