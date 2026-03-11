//! Dependency validation and cycle detection for task runtime

use super::types::TaskEntry;
use crate::error::{SupervisorError, ValidationError};
use std::collections::{HashMap, HashSet};

/// Validate the dependency graph:
/// - All declared dep IDs must correspond to a registered task
/// - No circular dependencies
pub fn validate_dependencies(tasks: &[TaskEntry]) -> Result<(), SupervisorError> {
    let known_ids: HashSet<&'static str> = tasks.iter().map(|e| e.task.id()).collect();

    // Check unknown IDs first
    for entry in tasks {
        for dep in entry.task.dependencies() {
            if !known_ids.contains(dep) {
                return Err(SupervisorError::dependency_validation(
                    entry.task.id(),
                    *dep,
                    ValidationError::UnknownTaskId,
                ));
            }
        }
    }

    // Cycle detection via DFS
    let graph: HashMap<&'static str, &[&'static str]> = tasks
        .iter()
        .map(|e| (e.task.id(), e.task.dependencies()))
        .collect();

    let mut visited: HashSet<&'static str> = HashSet::new();
    let mut stack: HashSet<&'static str> = HashSet::new();

    for id in graph.keys() {
        if !visited.contains(id) {
            dfs_cycle_check(id, &graph, &mut visited, &mut stack)?;
        }
    }

    Ok(())
}

fn dfs_cycle_check<'a>(
    node: &'a str,
    graph: &HashMap<&'a str, &[&'static str]>,
    visited: &mut HashSet<&'a str>,
    stack: &mut HashSet<&'a str>,
) -> Result<(), SupervisorError> {
    visited.insert(node);
    stack.insert(node);

    if let Some(deps) = graph.get(node) {
        for dep in *deps {
            if !visited.contains(dep) {
                dfs_cycle_check(dep, graph, visited, stack)?;
            } else if stack.contains(dep) {
                return Err(SupervisorError::circular_dependency(node, *dep));
            }
        }
    }

    stack.remove(node);
    Ok(())
}
