use crate::core::workflow::{Edge, Node, Workflow};
use std::collections::{HashMap, HashSet, VecDeque};

pub struct WorkflowGraph {
    nodes: HashMap<String, Node>,
    edges: Vec<Edge>,
    adjacency: HashMap<String, Vec<String>>,
    in_degree: HashMap<String, usize>,
}

impl WorkflowGraph {
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut nodes = HashMap::new();

        for node in &workflow.nodes {
            nodes.insert(node.id.clone(), node.clone());
            adjacency.insert(node.id.clone(), Vec::new());
            in_degree.insert(node.id.clone(), 0);
        }

        for edge in &workflow.edges {
            adjacency
                .get_mut(&edge.from)
                .unwrap()
                .push(edge.to.clone());
            
            *in_degree.get_mut(&edge.to).unwrap() += 1;
        }

        Self {
            nodes,
            edges: workflow.edges.clone(),
            adjacency,
            in_degree,
        }
    }

    pub fn get_execution_order(&self) -> Result<Vec<String>, String> {
        let mut queue = VecDeque::new();
        let mut in_degree = self.in_degree.clone();
        let mut result = Vec::new();

        for (node_id, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(node_id.clone());
            }
        }

        while let Some(node_id) = queue.pop_front() {
            result.push(node_id.clone());

            if let Some(neighbors) = self.adjacency.get(&node_id) {
                for neighbor in neighbors {
                    let degree = in_degree.get_mut(neighbor).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        if result.len() != self.nodes.len() {
            return Err("Workflow contains cycles".to_string());
        }

        Ok(result)
    }

    pub fn get_parallel_groups(&self) -> Result<Vec<Vec<String>>, String> {
        let mut groups = Vec::new();
        let mut processed = HashSet::new();
        let mut in_degree = self.in_degree.clone();

        loop {
            let mut current_group = Vec::new();

            for (node_id, &degree) in &in_degree {
                if degree == 0 && !processed.contains(node_id) {
                    current_group.push(node_id.clone());
                }
            }

            if current_group.is_empty() {
                break;
            }

            for node_id in &current_group {
                processed.insert(node_id.clone());
                
                if let Some(neighbors) = self.adjacency.get(node_id) {
                    for neighbor in neighbors {
                        *in_degree.get_mut(neighbor).unwrap() -= 1;
                    }
                }
            }

            groups.push(current_group);
        }

        if processed.len() != self.nodes.len() {
            return Err("Workflow contains cycles or unreachable nodes".to_string());
        }

        Ok(groups)
    }

    pub fn get_node(&self, node_id: &str) -> Option<&Node> {
        self.nodes.get(node_id)
    }

    pub fn get_dependencies(&self, node_id: &str) -> Vec<String> {
        let mut deps = Vec::new();
        for edge in &self.edges {
            if edge.to == node_id {
                deps.push(edge.from.clone());
            }
        }
        deps
    }

    pub fn get_dependents(&self, node_id: &str) -> Vec<String> {
        self.adjacency
            .get(node_id)
            .cloned()
            .unwrap_or_default()
    }
}