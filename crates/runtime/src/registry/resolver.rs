//! Dependency resolver for skills.
//!
//! Uses a topological sort to determine the correct installation order
//! for skills with dependencies.

use petgraph::algo::{is_cyclic_directed, toposort};
use petgraph::graph::{DiGraph, NodeIndex};
use std::collections::HashMap;
use thiserror::Error;

#[cfg(test)]
use crate::models::SkillDependency;
use crate::models::{SkillManifest, SkillVersion};

/// Errors that can occur during dependency resolution
#[derive(Debug, Error)]
pub enum DependencyError {
    #[error("Skill not found: {0}")]
    SkillNotFound(String),

    #[error("Version not found: {skill}@{version}")]
    VersionNotFound { skill: String, version: String },

    #[error("Circular dependency detected involving: {0}")]
    CircularDependency(String),

    #[error("Incompatible version requirements for {skill}: {existing} vs {new}")]
    IncompatibleVersions {
        skill: String,
        existing: String,
        new: String,
    },

    #[error("Resolution error: {0}")]
    Other(String),
}

/// An action in the install plan
#[derive(Debug, Clone)]
pub enum InstallAction {
    /// Install a new skill
    Install {
        skill_id: String,
        version: SkillVersion,
    },
    /// Update an existing skill
    Update {
        skill_id: String,
        from_version: SkillVersion,
        to_version: SkillVersion,
    },
    /// Skip (already installed and up-to-date)
    Skip { skill_id: String },
}

/// A plan for installing skills with dependencies
#[derive(Debug, Clone)]
pub struct InstallPlan {
    /// Ordered list of actions to perform
    pub actions: Vec<InstallAction>,
    /// Skills that will be installed (for summary)
    pub to_install: Vec<String>,
    /// Skills that will be updated (for summary)
    pub to_update: Vec<String>,
    /// Skills that are already installed (for summary)
    pub unchanged: Vec<String>,
}

impl InstallPlan {
    /// Create an empty plan
    pub fn empty() -> Self {
        Self {
            actions: Vec::new(),
            to_install: Vec::new(),
            to_update: Vec::new(),
            unchanged: Vec::new(),
        }
    }

    /// Check if the plan has any actions
    pub fn has_actions(&self) -> bool {
        !self.to_install.is_empty() || !self.to_update.is_empty()
    }
}

/// Dependency resolver
pub struct DependencyResolver {
    /// Graph for dependency tracking
    graph: DiGraph<String, ()>,
    /// Map from skill ID to node index
    node_map: HashMap<String, NodeIndex>,
    /// Resolved manifests
    manifests: HashMap<String, SkillManifest>,
    /// Currently installed skills
    installed: HashMap<String, SkillVersion>,
}

impl DependencyResolver {
    /// Create a new resolver
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
            manifests: HashMap::new(),
            installed: HashMap::new(),
        }
    }

    /// Set the currently installed skills
    pub fn set_installed(&mut self, installed: HashMap<String, SkillVersion>) {
        self.installed = installed;
    }

    /// Add a skill to the resolution graph
    pub fn add_skill(&mut self, manifest: SkillManifest) -> Result<(), DependencyError> {
        let skill_id = manifest.id.clone();

        // Get or create node
        let node = if let Some(&existing) = self.node_map.get(&skill_id) {
            existing
        } else {
            let idx = self.graph.add_node(skill_id.clone());
            self.node_map.insert(skill_id.clone(), idx);
            idx
        };

        // Add dependencies
        for dep in &manifest.dependencies {
            if dep.optional {
                continue; // Skip optional dependencies for now
            }

            // Get or create dependency node
            let dep_node = if let Some(&existing) = self.node_map.get(&dep.skill_id) {
                existing
            } else {
                let idx = self.graph.add_node(dep.skill_id.clone());
                self.node_map.insert(dep.skill_id.clone(), idx);
                idx
            };

            // Add edge from skill to dependency (skill depends on dep)
            self.graph.add_edge(node, dep_node, ());
        }

        self.manifests.insert(skill_id, manifest);

        Ok(())
    }

    /// Resolve dependencies and create an install plan
    pub fn resolve(&self, root_skills: &[String]) -> Result<InstallPlan, DependencyError> {
        // Check for cycles
        if is_cyclic_directed(&self.graph) {
            // Find a cycle for error reporting
            let cycle_skill = self
                .node_map
                .keys()
                .next()
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            return Err(DependencyError::CircularDependency(cycle_skill));
        }

        // Topological sort (reversed because we want dependencies first)
        let sorted = toposort(&self.graph, None).map_err(|e| {
            let skill = self
                .graph
                .node_weight(e.node_id())
                .cloned()
                .unwrap_or_else(|| "unknown".to_string());
            DependencyError::CircularDependency(skill)
        })?;

        let mut plan = InstallPlan::empty();

        // Process in reverse order (dependencies first)
        for node in sorted.into_iter().rev() {
            let skill_id = match self.graph.node_weight(node) {
                Some(id) => id.clone(),
                None => continue,
            };

            // Skip if not in the transitive closure of root skills
            if !self.is_reachable_from_roots(&skill_id, root_skills) {
                continue;
            }

            let manifest = match self.manifests.get(&skill_id) {
                Some(m) => m,
                None => {
                    // Dependency not yet added to resolver
                    plan.actions.push(InstallAction::Install {
                        skill_id: skill_id.clone(),
                        version: SkillVersion::default(),
                    });
                    plan.to_install.push(skill_id);
                    continue;
                }
            };

            // Check if already installed
            if let Some(installed_version) = self.installed.get(&skill_id) {
                if installed_version == &manifest.version {
                    // Already at correct version
                    plan.actions.push(InstallAction::Skip {
                        skill_id: skill_id.clone(),
                    });
                    plan.unchanged.push(skill_id);
                } else {
                    // Needs update
                    plan.actions.push(InstallAction::Update {
                        skill_id: skill_id.clone(),
                        from_version: installed_version.clone(),
                        to_version: manifest.version.clone(),
                    });
                    plan.to_update.push(skill_id);
                }
            } else {
                // New installation
                plan.actions.push(InstallAction::Install {
                    skill_id: skill_id.clone(),
                    version: manifest.version.clone(),
                });
                plan.to_install.push(skill_id);
            }
        }

        Ok(plan)
    }

    /// Check if a skill is reachable from the root skills
    fn is_reachable_from_roots(&self, skill_id: &str, root_skills: &[String]) -> bool {
        if root_skills.contains(&skill_id.to_string()) {
            return true;
        }

        // Check if any root skill depends on this skill (transitively)
        for root in root_skills {
            if self.is_dependency_of(skill_id, root) {
                return true;
            }
        }

        false
    }

    /// Check if skill_id is a dependency of root_id
    fn is_dependency_of(&self, skill_id: &str, root_id: &str) -> bool {
        let root_node = match self.node_map.get(root_id) {
            Some(&n) => n,
            None => return false,
        };

        let skill_node = match self.node_map.get(skill_id) {
            Some(&n) => n,
            None => return false,
        };

        // BFS to check reachability
        use petgraph::visit::Bfs;
        let mut bfs = Bfs::new(&self.graph, root_node);
        while let Some(node) = bfs.next(&self.graph) {
            if node == skill_node {
                return true;
            }
        }

        false
    }

    /// Get the resolved manifests
    pub fn manifests(&self) -> &HashMap<String, SkillManifest> {
        &self.manifests
    }

    /// Clear the resolver state
    pub fn clear(&mut self) {
        self.graph.clear();
        self.node_map.clear();
        self.manifests.clear();
    }
}

impl Default for DependencyResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_manifest(id: &str, deps: Vec<&str>) -> SkillManifest {
        SkillManifest {
            id: id.to_string(),
            name: id.to_string(),
            version: SkillVersion::new(1, 0, 0),
            dependencies: deps
                .into_iter()
                .map(|d| SkillDependency {
                    skill_id: d.to_string(),
                    version: crate::models::VersionRequirement::Any,
                    optional: false,
                })
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn test_simple_resolution() {
        let mut resolver = DependencyResolver::new();

        // A depends on B, B depends on C
        resolver
            .add_skill(create_manifest("skill-a", vec!["skill-b"]))
            .unwrap();
        resolver
            .add_skill(create_manifest("skill-b", vec!["skill-c"]))
            .unwrap();
        resolver
            .add_skill(create_manifest("skill-c", vec![]))
            .unwrap();

        let plan = resolver.resolve(&["skill-a".to_string()]).unwrap();

        // Should install C, then B, then A
        assert_eq!(plan.to_install.len(), 3);

        // Find positions
        let pos_a = plan.to_install.iter().position(|s| s == "skill-a").unwrap();
        let pos_b = plan.to_install.iter().position(|s| s == "skill-b").unwrap();
        let pos_c = plan.to_install.iter().position(|s| s == "skill-c").unwrap();

        // Dependencies should come before dependents
        assert!(pos_c < pos_b);
        assert!(pos_b < pos_a);
    }

    #[test]
    fn test_already_installed() {
        let mut resolver = DependencyResolver::new();

        let mut installed = HashMap::new();
        installed.insert("skill-b".to_string(), SkillVersion::new(1, 0, 0));
        resolver.set_installed(installed);

        resolver
            .add_skill(create_manifest("skill-a", vec!["skill-b"]))
            .unwrap();
        resolver
            .add_skill(create_manifest("skill-b", vec![]))
            .unwrap();

        let plan = resolver.resolve(&["skill-a".to_string()]).unwrap();

        assert_eq!(plan.to_install.len(), 1);
        assert_eq!(plan.to_install[0], "skill-a");
        assert_eq!(plan.unchanged.len(), 1);
        assert_eq!(plan.unchanged[0], "skill-b");
    }

    #[test]
    fn test_circular_dependency() {
        let mut resolver = DependencyResolver::new();

        resolver
            .add_skill(create_manifest("skill-a", vec!["skill-b"]))
            .unwrap();
        resolver
            .add_skill(create_manifest("skill-b", vec!["skill-a"]))
            .unwrap();

        let result = resolver.resolve(&["skill-a".to_string()]);
        assert!(matches!(
            result,
            Err(DependencyError::CircularDependency(_))
        ));
    }

    #[test]
    fn test_diamond_dependency() {
        let mut resolver = DependencyResolver::new();

        // A depends on B and C, both B and C depend on D
        resolver
            .add_skill(create_manifest("skill-a", vec!["skill-b", "skill-c"]))
            .unwrap();
        resolver
            .add_skill(create_manifest("skill-b", vec!["skill-d"]))
            .unwrap();
        resolver
            .add_skill(create_manifest("skill-c", vec!["skill-d"]))
            .unwrap();
        resolver
            .add_skill(create_manifest("skill-d", vec![]))
            .unwrap();

        let plan = resolver.resolve(&["skill-a".to_string()]).unwrap();

        // All 4 skills should be installed
        assert_eq!(plan.to_install.len(), 4);

        // D should come before B and C
        let pos_d = plan.to_install.iter().position(|s| s == "skill-d").unwrap();
        let pos_b = plan.to_install.iter().position(|s| s == "skill-b").unwrap();
        let pos_c = plan.to_install.iter().position(|s| s == "skill-c").unwrap();
        let pos_a = plan.to_install.iter().position(|s| s == "skill-a").unwrap();

        assert!(pos_d < pos_b);
        assert!(pos_d < pos_c);
        assert!(pos_b < pos_a);
        assert!(pos_c < pos_a);
    }
}
