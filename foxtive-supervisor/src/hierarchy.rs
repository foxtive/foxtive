//! Supervisor hierarchy and nesting support
//!
//! This module provides the ability to create parent-child relationships between supervisors,
//! enabling hierarchical task management and organized supervision trees.

use crate::runtime::TaskRuntime;
use crate::Supervisor;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Builder for constructing supervisor hierarchies
pub struct SupervisorHierarchy {
    root: HierarchyBuilder,
}

/// Internal builder node - mutable during construction
struct HierarchyBuilder {
    supervisor: Option<Supervisor>,
    children: Vec<HierarchyBuilder>,
    id: String,
}

/// Running hierarchy - immutable tree of TaskRuntimes
pub struct HierarchyRuntime {
    root: Arc<RuntimeNode>,
}

/// A node in the running hierarchy tree
pub struct RuntimeNode {
    runtime: Mutex<Option<TaskRuntime>>,
    children: Vec<Arc<RuntimeNode>>,
    id: String,
}

impl HierarchyBuilder {
    fn new(id: impl Into<String>) -> Self {
        Self {
            supervisor: None,
            children: Vec::new(),
            id: id.into(),
        }
    }

    fn with_supervisor(mut self, supervisor: Supervisor) -> Self {
        self.supervisor = Some(supervisor);
        self
    }

    fn add_child(mut self, child: HierarchyBuilder) -> Self {
        self.children.push(child);
        self
    }
}

impl SupervisorHierarchy {
    /// Create a new supervisor hierarchy with a root node
    pub fn new(root_id: impl Into<String>) -> Self {
        Self {
            root: HierarchyBuilder::new(root_id),
        }
    }

    /// Add a child supervisor at the root level
    pub fn add_child(mut self, id: impl Into<String>, supervisor: Supervisor) -> Self {
        let child = HierarchyBuilder::new(id).with_supervisor(supervisor);
        self.root = self.root.add_child(child);
        self
    }

    /// Start all supervisors in the hierarchy (bottom-up)
    ///
    /// This consumes the builder and returns a running hierarchy.
    /// Children are started before their parents to ensure dependencies are ready.
    pub async fn start_all(self) -> Result<HierarchyRuntime, crate::error::SupervisorError> {
        info!("Starting hierarchy from root '{}'", self.root.id);
        let root = Box::pin(Self::start_builder_node(self.root)).await?;
        Ok(HierarchyRuntime { root })
    }

    /// Recursively start a builder node and its children
    async fn start_builder_node(
        builder: HierarchyBuilder,
    ) -> Result<Arc<RuntimeNode>, crate::error::SupervisorError> {
        // Start children first (bottom-up)
        let mut children = Vec::new();
        for child_builder in builder.children {
            let child_runtime = Box::pin(Self::start_builder_node(child_builder)).await?;
            children.push(child_runtime);
        }

        // Start this supervisor if it has one
        let runtime = if let Some(supervisor) = builder.supervisor {
            info!(node_id = %builder.id, "Starting supervisor");
            let rt = supervisor.start().await?;
            info!(node_id = %builder.id, "Supervisor started with {} tasks", rt.task_count());
            Some(rt)
        } else {
            None
        };

        Ok(Arc::new(RuntimeNode {
            runtime: Mutex::new(runtime),
            children,
            id: builder.id,
        }))
    }
}

impl RuntimeNode {
    /// Shutdown this node and all its children
    async fn shutdown_node(self: Arc<Self>) {
        info!(node_id = %self.id, "Initiating shutdown");

        // Shutdown children in parallel
        let shutdown_futures: Vec<_> = self.children.iter().map(|child| {
            let child_clone = child.clone();
            async move {
                Box::pin(child_clone.shutdown_node()).await;
            }
        }).collect();

        futures::future::join_all(shutdown_futures).await;

        // Shutdown this node's runtime
        let mut guard = self.runtime.lock().await;
        if let Some(runtime) = guard.take() {
            info!(node_id = %self.id, "Shutting down supervisor");
            runtime.shutdown().await;
            info!(node_id = %self.id, "Supervisor shut down");
        }
    }

    /// Get total task count for this node and all descendants
    fn task_count(&self) -> usize {
        let local_count = match &*self.runtime.try_lock().unwrap() {
            Some(rt) => rt.task_count(),
            None => 0,
        };

        let child_count: usize = self.children.iter()
            .map(|child| child.task_count())
            .sum();

        local_count + child_count
    }

    /// Find a node by ID
    #[allow(dead_code)]
    fn find_node(&self, target_id: &str) -> Option<&Self> {
        if self.id == target_id {
            return Some(self);
        }

        for child in &self.children {
            if let Some(found) = child.find_node(target_id) {
                return Some(found);
            }
        }

        None
    }
}

impl HierarchyRuntime {
    /// Shutdown all supervisors in the hierarchy (top-down)
    pub async fn shutdown_all(self) {
        info!("Initiating hierarchy shutdown from root '{}'", self.root.id);
        Box::pin(self.root.shutdown_node()).await;
        info!("Hierarchy shutdown complete");
    }

    /// Get total task count across all nodes
    pub fn total_task_count(&self) -> usize {
        self.root.task_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_hierarchy() {
        let hierarchy = SupervisorHierarchy::new("root");
        // Just verify it compiles and creates
        let _ = hierarchy;
    }
}
