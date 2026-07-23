//! Dependency graph resolution for service initialization.
//!
//! Uses on-demand depth-first search (DFS) to determine the correct
//! construction order for services based on their declared dependencies.
//! Detects circular dependencies and provides helpful error messages
//! that include the full dependency chain.
//!
//! # Unresolved Dependencies
//!
//! If a dependency name doesn't match any registered factory, a debug-level
//! log is emitted. This is intentional - the dependency may be pre-registered
//! via `AppInit::register()` or be an external type. Use `strict_deps()` on
//! the builder to turn these into errors.
//!
//! # Two-Phase Construction
//!
//! Phase 1a (this module) performs a synchronous DFS to produce a flat
//! construction order. Phase 1b (in `freeze()`) constructs services in that
//! order. If a service fails with `ServiceResolutionError::DependencyMissing`
//! (undeclared runtime dep), it is deferred to Phase 2 - a single retry pass
//! that runs after all declared-dep services are in the TypeMap.

use crate::app::di_error::{compute_blocked_services, short_type_name, DiError};
use crate::enums::AppMessage;
use crate::lifecycle::ServiceFactory;
use crate::results::AppResult;
use std::collections::{HashMap, HashSet};

/// Internal error type for service construction during `freeze()`.
///
/// Separates retryable dependency resolution failures from terminal failures.
/// This is `pub(crate)` - it never appears in the public API. User code sees
/// `AppResult` (i.e., `Result<T, AppMessage>`) as before.
#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum ServiceResolutionError {
    /// A dependency was not yet in the TypeMap - retryable in Phase 2.
    ///
    /// Produced when `app.require::<T>()` fails inside `T::init()` because the
    /// dependency hasn't been constructed yet. Phase 2 will retry after other
    /// services are constructed.
    DependencyMissing {
        /// The service that failed to construct (for diagnostics).
        service: &'static str,
        /// The formatted error message from `app.require()` (e.g.,
        /// "Service of type X not registered").
        missing_type: String,
    },

    /// Terminal failure - not retryable, propagate immediately.
    ///
    /// Produced for all other errors: DB connection failures, config errors,
    /// I/O errors, etc. These indicate genuine runtime issues that retry
    /// cannot fix.
    Terminal(AppMessage),
}

/// Try exact match first, then fall back to suffix matching.
///
/// Suffix matching handles the case where a manually-written `dependencies()`
/// returns a short type name (e.g. `"MaintenanceService"`) while the factory
/// is registered with a fully-qualified name
/// (e.g. `"my_crate::services::MaintenanceService"`).
///
/// If the suffix match is ambiguous (multiple factories share the same
/// trailing name), we log a warning and return `None`.
fn match_exact_or_suffix(
    dep_name: &str,
    type_to_idx: &HashMap<&str, usize>,
    requester: &str,
) -> Option<usize> {
    // Fast path: exact match
    if let Some(&idx) = type_to_idx.get(dep_name) {
        return Some(idx);
    }

    // Slow path: suffix match - dep_name matches the tail of a registered name
    // after a `::` boundary to avoid false positives (e.g. "Service" matching "MyService").
    let candidates: Vec<(&str, usize)> = type_to_idx
        .iter()
        .filter(|(name, _)| {
            name.ends_with(dep_name)
                && name.len() > dep_name.len()
                && name.as_bytes()[name.len() - dep_name.len() - 1] == b':'
        })
        .map(|(name, &idx)| (*name, idx))
        .collect();

    match candidates.len() {
        0 => None,
        1 => {
            let (full_name, idx) = candidates[0];
            tracing::warn!(
                service = requester,
                declared_dep = dep_name,
                resolved_to = full_name,
                "Dependency matched by suffix - use std::any::type_name::<T>() in dependencies() for exact matching"
            );
            Some(idx)
        }
        _ => {
            let names: Vec<&str> = candidates.iter().map(|(n, _)| *n).collect();
            tracing::warn!(
                service = requester,
                declared_dep = dep_name,
                ambiguous_matches = ?names,
                "Ambiguous suffix match for dependency - skipping graph edge"
            );
            None
        }
    }
}

/// Resolves the construction order for services based on their declared dependencies.
///
/// Uses on-demand DFS to determine the correct order. Detects circular
/// dependencies and provides helpful error messages including the full
/// dependency chain. Preserves registration order when there are no
/// dependency constraints.
///
/// Returns a flat `Vec<usize>` of factory indices in valid construction order
/// (dependencies before dependents). The caller (`freeze()`) iterates this
/// order sequentially, calling `factory.create()` for each.
pub(crate) fn resolve_construction_order(
    factories: &[Box<dyn ServiceFactory>],
) -> AppResult<Vec<usize>> {
    // Build a map from type name to index
    let type_to_idx: HashMap<&str, usize> = factories
        .iter()
        .enumerate()
        .map(|(i, f)| (f.type_name(), i))
        .collect();

    let n = factories.len();
    // Fully-resolved nodes (post-order added)
    let mut visited: HashSet<usize> = HashSet::with_capacity(n);
    // Currently-being-visited stack (for cycle detection)
    let mut visiting: Vec<usize> = Vec::new();
    let mut order: Vec<usize> = Vec::with_capacity(n);

    for idx in 0..n {
        if !visited.contains(&idx) {
            visit(
                idx,
                factories,
                &type_to_idx,
                &mut visited,
                &mut visiting,
                &mut order,
            )?;
        }
    }

    Ok(order)
}

/// Recursive DFS visitor.
///
/// Post-order traversal: a node is added to `order` only after all its
/// dependencies have been visited. The `visiting` stack tracks the current
/// DFS path for cycle detection - if we encounter a node already in
/// `visiting`, we have a cycle and can extract the full chain.
fn visit(
    idx: usize,
    factories: &[Box<dyn ServiceFactory>],
    type_to_idx: &HashMap<&str, usize>,
    visited: &mut HashSet<usize>,
    visiting: &mut Vec<usize>,
    order: &mut Vec<usize>,
) -> AppResult<()> {
    if visited.contains(&idx) {
        return Ok(());
    }

    if let Some(cycle_start) = visiting.iter().position(|&v| v == idx) {
        // Build cycle chain from stack using short type names
        let chain: Vec<String> = visiting[cycle_start..]
            .iter()
            .map(|&i| short_type_name(factories[i].type_name()).to_string())
            .chain(std::iter::once(
                short_type_name(factories[idx].type_name()).to_string(),
            ))
            .collect();

        return Err(DiError::DeclaredCircularDependency { chain }.into());
    }

    visiting.push(idx);

    // Recursively visit declared dependencies first
    for dep_name in factories[idx].dependencies() {
        if let Some(dep_idx) =
            match_exact_or_suffix(dep_name, type_to_idx, factories[idx].type_name())
        {
            visit(dep_idx, factories, type_to_idx, visited, visiting, order)?;
        } else {
            // Dependency not found in factories - it may be pre-registered
            // via register() or be an external type. Log for debugging.
        }
    }

    visiting.pop();
    visited.insert(idx);
    order.push(idx);
    Ok(())
}

/// Format a deadlock error from Phase 2 captured errors.
///
/// Called when Phase 2 makes no progress in a full pass - meaning the
/// remaining candidates have undeclared circular dependencies or reference
/// types that were never registered.
///
/// This function builds a complete dependency graph from ALL blocked services
/// (not just one edge per service) and uses DFS to detect cycles. When a
/// cycle is found, the error includes the cycle chain and suggests which
/// specific dependencies to wrap in `Lazy<T>`.
///
/// Cycles are deduplicated (same cycle reported from different entry points
/// is shown only once) and type names are shortened to just the struct name
/// (e.g., `AuthService` instead of `ocean::services::auth_service::AuthService`).
///
/// Returns `DiError` - callers in `init.rs` convert to `AppMessage` via `From`.
pub(crate) fn format_deadlock_error(
    last_errors: &HashMap<usize, ServiceResolutionError>,
    factories: &[Box<dyn ServiceFactory>],
    constructed_set: &HashSet<usize>,
) -> DiError {
    // Build COMPLETE "needs" graph: candidate_idx → Set of factory_idxs it depends on.
    //
    // We use THREE sources to capture ALL edges:
    //   1. last_errors: the ONE runtime missing dep per service (from Phase 2 retry)
    //   2. declared dependencies(): all declared deps that are also failed candidates
    //   3. compute_blocked_services: the COMPLETE blocked deps list (both declared
    //      and runtime) which we use to build reverse name→idx mappings
    //
    // Without sources #2 and #3, cycles are missed when a service has multiple
    // missing deps but last_errors only captured the last one tried.
    let candidate_set: HashSet<usize> = last_errors.keys().copied().collect();
    let mut needs_graph: HashMap<usize, HashSet<usize>> = HashMap::new();

    // Build short_name → candidate_idx map for resolving blocked service names
    let short_name_to_candidate: HashMap<&str, usize> = candidate_set
        .iter()
        .map(|&idx| (short_type_name(factories[idx].type_name()), idx))
        .collect();

    // Source 3 (PRIMARY): Use compute_blocked_services to get ALL blocked deps
    // per service, then build graph edges from those that are also candidates.
    let (blocked_services, _) =
        compute_blocked_services(last_errors, factories, &HashSet::new(), constructed_set);
    for (service_short, needs_short) in &blocked_services {
        if let Some(&from_idx) = short_name_to_candidate.get(service_short.as_str())
            && let Some(&to_idx) = short_name_to_candidate.get(needs_short.as_str())
            && from_idx != to_idx
        {
            needs_graph.entry(from_idx).or_default().insert(to_idx);
        }
    }

    // Source 1: Runtime missing deps from last_errors (catches undeclared deps
    // not visible to compute_blocked_services)
    for (&idx, err) in last_errors {
        if let ServiceResolutionError::DependencyMissing { missing_type, .. } = err {
            let type_name = crate::app::di_error::parse_missing_type_name(missing_type);
            if let Some(target) = find_factory_by_type_name(factories, &type_name)
                && candidate_set.contains(&target)
            {
                needs_graph.entry(idx).or_default().insert(target);
            }
        }
    }

    // Source 2: Declared dependencies that are also failed candidates.
    for &idx in &candidate_set {
        for dep_name in factories[idx].dependencies() {
            if let Some(dep_idx) = find_factory_by_type_name(factories, dep_name)
                && candidate_set.contains(&dep_idx)
                && dep_idx != idx
            {
                needs_graph.entry(idx).or_default().insert(dep_idx);
            }
        }
    }

    // Detect cycles using DFS on the complete needs graph
    let mut unique_cycles: Vec<Vec<usize>> = Vec::new();
    let mut seen_normalized: HashSet<Vec<usize>> = HashSet::new();
    let mut globally_visited: HashSet<usize> = HashSet::new();

    for &start in needs_graph.keys() {
        if globally_visited.contains(&start) {
            continue;
        }
        let mut path = Vec::new();
        let mut path_set = HashSet::new();
        let mut local_visited = HashSet::new();

        dfs_find_cycles(
            start,
            &needs_graph,
            &mut path,
            &mut path_set,
            &mut local_visited,
            &mut unique_cycles,
            &mut seen_normalized,
        );

        globally_visited.extend(local_visited);
    }

    // Convert cycles to short-name format
    let cycle_members: HashSet<usize> = unique_cycles
        .iter()
        .flat_map(|c| c.iter().copied())
        .collect();

    let cycles_str: Vec<Vec<String>> = unique_cycles
        .iter()
        .map(|cycle| {
            cycle
                .iter()
                .map(|&idx| short_type_name(factories[idx].type_name()).to_string())
                .collect()
        })
        .collect();

    // Compute blocked services and terminal errors
    let (blocked_services, _terminal_errors) =
        compute_blocked_services(last_errors, factories, &cycle_members, constructed_set);

    // Compute unregistered deps: blocked services that reference types NOT in factories at all.
    // These are root-cause failures - registering these types would unblock the chain.
    let all_factory_type_names: HashSet<&str> = factories.iter().map(|f| f.type_name()).collect();
    let mut unregistered_deps: Vec<(String, String)> = Vec::new();
    for (&idx, err) in last_errors {
        if cycle_members.contains(&idx) {
            continue;
        }
        let service_name = short_type_name(factories[idx].type_name()).to_string();
        if let ServiceResolutionError::DependencyMissing { missing_type, .. } = err {
            let missing_fq = crate::app::di_error::parse_missing_type_name(missing_type);
            // Check if this dep is registered at all
            if !all_factory_type_names.contains(missing_fq.as_str())
                && !find_factory_by_type_name(factories, &missing_fq).is_some()
            {
                let dep_short = short_type_name(&missing_fq).to_string();
                unregistered_deps.push((service_name.clone(), dep_short));
            }
        }
        // Also check declared deps that aren't in factories
        for dep_name in factories[idx].dependencies() {
            if !all_factory_type_names.contains(dep_name)
                && !find_factory_by_type_name(factories, dep_name).is_some()
            {
                let dep_short = short_type_name(dep_name).to_string();
                unregistered_deps.push((service_name.clone(), dep_short));
            }
        }
    }
    unregistered_deps.sort();
    unregistered_deps.dedup();

    DiError::CircularRuntimeDependency {
        cycles: cycles_str,
        blocked_services,
        unregistered_deps,
    }
}

/// Normalize a cycle for deduplication: rotate so the smallest index
/// comes first. This ensures [A,B,C] and [B,C,A] produce the same key.
fn normalize_cycle(cycle: &[usize]) -> Vec<usize> {
    if cycle.is_empty() {
        return Vec::new();
    }
    let min_val = *cycle.iter().min().unwrap();
    let min_pos = cycle.iter().position(|&v| v == min_val).unwrap();
    let mut normalized = Vec::with_capacity(cycle.len());
    for i in 0..cycle.len() {
        normalized.push(cycle[(min_pos + i) % cycle.len()]);
    }
    normalized
}

/// Find a factory index by its type name, using exact match first
/// then suffix match (same logic as the DFS resolver).
fn find_factory_by_type_name(
    factories: &[Box<dyn ServiceFactory>],
    type_name: &str,
) -> Option<usize> {
    // Exact match
    if let Some(idx) = factories.iter().position(|f| f.type_name() == type_name) {
        return Some(idx);
    }
    // Suffix match (bounded by `::`)
    let matches: Vec<usize> = factories
        .iter()
        .enumerate()
        .filter(|(_, f)| {
            let name = f.type_name();
            name.ends_with(type_name)
                && name.len() > type_name.len()
                && name.as_bytes()[name.len() - type_name.len() - 1] == b':'
        })
        .map(|(i, _)| i)
        .collect();
    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

/// DFS-based cycle detection on a multi-edge graph.
///
/// Traverses the `needs_graph` from `node`, tracking the current path.
/// When a back-edge is found (next node is already on the current path),
/// the cycle is extracted, normalized, and deduplicated before being
/// added to `unique_cycles`.
fn dfs_find_cycles(
    node: usize,
    needs_graph: &HashMap<usize, HashSet<usize>>,
    path: &mut Vec<usize>,
    path_set: &mut HashSet<usize>,
    visited: &mut HashSet<usize>,
    unique_cycles: &mut Vec<Vec<usize>>,
    seen_normalized: &mut HashSet<Vec<usize>>,
) {
    if path_set.contains(&node) {
        // Back-edge found - extract the cycle portion
        if let Some(pos) = path.iter().position(|&n| n == node) {
            let cycle = path[pos..].to_vec();
            let normalized = normalize_cycle(&cycle);
            if seen_normalized.insert(normalized) {
                unique_cycles.push(cycle);
            }
        }
        return;
    }
    if visited.contains(&node) {
        return;
    }

    path.push(node);
    path_set.insert(node);

    if let Some(neighbors) = needs_graph.get(&node) {
        for &next in neighbors {
            dfs_find_cycles(
                next,
                needs_graph,
                path,
                path_set,
                visited,
                unique_cycles,
                seen_normalized,
            );
        }
    }

    path.pop();
    path_set.remove(&node);
    visited.insert(node);
}

/// Trace a "needs" chain starting from `start` to detect a cycle (legacy single-edge version).
///
/// Follows `needs_map` (candidate → the other candidate it couldn't find).
/// Returns `Some(cycle_chain)` if the chain loops back on itself.
/// The chain starts and ends with the cycle members (not the repeated node).
#[allow(dead_code)] // Used by unit tests
fn trace_cycle_chain(start: usize, needs_map: &HashMap<usize, usize>) -> Option<Vec<usize>> {
    let mut chain = vec![start];
    let mut seen: HashSet<usize> = [start].into();
    let mut current = start;

    while let Some(&next) = needs_map.get(&current) {
        if seen.contains(&next) {
            // Found a cycle - extract just the cycle portion
            if let Some(pos) = chain.iter().position(|&n| n == next) {
                return Some(chain[pos..].to_vec());
            }
            return None;
        }
        seen.insert(next);
        chain.push(next);
        current = next;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::App;
    use std::future::Future;
    use std::pin::Pin;

    struct MockFactory {
        name: &'static str,
        deps: Vec<&'static str>,
    }

    impl ServiceFactory for MockFactory {
        fn type_name(&self) -> &'static str {
            self.name
        }

        fn dependencies(&self) -> &[&'static str] {
            &self.deps
        }

        fn create(
            &self,
            _app: &App,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            Box<dyn std::any::Any + Send + Sync>,
                            ServiceResolutionError,
                        >,
                    > + Send,
            >,
        > {
            Box::pin(async { Ok(Box::new(()) as Box<dyn std::any::Any + Send + Sync>) })
        }
    }

    #[test]
    fn test_dfs_no_dependencies() {
        let factories: Vec<Box<dyn ServiceFactory>> = vec![
            Box::new(MockFactory {
                name: "A",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "B",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "C",
                deps: vec![],
            }),
        ];

        let order = resolve_construction_order(&factories).unwrap();
        assert_eq!(order.len(), 3);
        // DFS with no deps preserves registration order
        assert_eq!(order, vec![0, 1, 2]);
    }

    #[test]
    fn test_dfs_linear_dependencies() {
        // A -> B -> C (C depends on B, B depends on A)
        let factories: Vec<Box<dyn ServiceFactory>> = vec![
            Box::new(MockFactory {
                name: "A",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "B",
                deps: vec!["A"],
            }),
            Box::new(MockFactory {
                name: "C",
                deps: vec!["B"],
            }),
        ];

        let order = resolve_construction_order(&factories).unwrap();
        let a_pos = order
            .iter()
            .position(|&i| factories[i].type_name() == "A")
            .unwrap();
        let b_pos = order
            .iter()
            .position(|&i| factories[i].type_name() == "B")
            .unwrap();
        let c_pos = order
            .iter()
            .position(|&i| factories[i].type_name() == "C")
            .unwrap();

        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn test_dfs_circular_dependency() {
        // A -> B -> A (circular)
        let factories: Vec<Box<dyn ServiceFactory>> = vec![
            Box::new(MockFactory {
                name: "A",
                deps: vec!["B"],
            }),
            Box::new(MockFactory {
                name: "B",
                deps: vec!["A"],
            }),
        ];

        let result = resolve_construction_order(&factories);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("DI0002") || err.to_string().contains("circular dependency")
        );
    }

    #[test]
    fn test_dfs_diamond_dependency() {
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        let factories: Vec<Box<dyn ServiceFactory>> = vec![
            Box::new(MockFactory {
                name: "A",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "B",
                deps: vec!["A"],
            }),
            Box::new(MockFactory {
                name: "C",
                deps: vec!["A"],
            }),
            Box::new(MockFactory {
                name: "D",
                deps: vec!["B", "C"],
            }),
        ];

        let order = resolve_construction_order(&factories).unwrap();
        let a_pos = order
            .iter()
            .position(|&i| factories[i].type_name() == "A")
            .unwrap();
        let b_pos = order
            .iter()
            .position(|&i| factories[i].type_name() == "B")
            .unwrap();
        let c_pos = order
            .iter()
            .position(|&i| factories[i].type_name() == "C")
            .unwrap();
        let d_pos = order
            .iter()
            .position(|&i| factories[i].type_name() == "D")
            .unwrap();

        assert!(a_pos < b_pos);
        assert!(a_pos < c_pos);
        assert!(b_pos < d_pos);
        assert!(c_pos < d_pos);
    }

    #[test]
    fn test_dfs_suffix_match() {
        // Simulates the real-world case where dependencies() returns short
        // type names but factories are registered with fully-qualified names.
        let factories: Vec<Box<dyn ServiceFactory>> = vec![
            Box::new(MockFactory {
                name: "crate::services::invoice_levy_service::InvoiceLevyService",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "crate::services::invoice_service::InvoiceService",
                deps: vec!["MaintenanceService"], // short name
            }),
            Box::new(MockFactory {
                name: "crate::services::maintenance_service::MaintenanceService",
                deps: vec![],
            }),
        ];

        let order = resolve_construction_order(&factories).unwrap();
        let inv_pos = order
            .iter()
            .position(|&i| {
                factories[i].type_name() == "crate::services::invoice_service::InvoiceService"
            })
            .unwrap();
        let maint_pos = order
            .iter()
            .position(|&i| {
                factories[i].type_name()
                    == "crate::services::maintenance_service::MaintenanceService"
            })
            .unwrap();

        // MaintenanceService must be constructed before InvoiceService
        assert!(
            maint_pos < inv_pos,
            "MaintenanceService should be initialized before InvoiceService"
        );
    }

    #[test]
    fn test_dfs_ambiguous_suffix() {
        // Two types ending with the same short name - suffix match is ambiguous
        let factories: Vec<Box<dyn ServiceFactory>> = vec![
            Box::new(MockFactory {
                name: "crate::a::FooService",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "crate::b::FooService",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "crate::c::Consumer",
                deps: vec!["FooService"], // ambiguous - matches both
            }),
        ];

        // Should still succeed (no edge added for ambiguous dep)
        let order = resolve_construction_order(&factories).unwrap();
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn test_trace_cycle_chain_simple() {
        // A needs B, B needs A → cycle [A, B]
        let needs_map: HashMap<usize, usize> = [(0, 1), (1, 0)].into();
        let cycle = trace_cycle_chain(0, &needs_map).unwrap();
        assert_eq!(cycle, vec![0, 1]);
    }

    #[test]
    fn test_trace_cycle_chain_triangle() {
        // A→B→C→A → cycle [A, B, C]
        let needs_map: HashMap<usize, usize> = [(0, 1), (1, 2), (2, 0)].into();
        let cycle = trace_cycle_chain(0, &needs_map).unwrap();
        assert_eq!(cycle, vec![0, 1, 2]);
    }

    #[test]
    fn test_trace_cycle_chain_no_cycle() {
        // A→B→C (dead end, C not in needs_map)
        let needs_map: HashMap<usize, usize> = [(0, 1), (1, 2)].into();
        assert!(trace_cycle_chain(0, &needs_map).is_none());
    }

    #[test]
    fn test_trace_cycle_chain_lead_in() {
        // D→A→B→C→A - lead-in D, cycle is [A, B, C]
        let needs_map: HashMap<usize, usize> = [(3, 0), (0, 1), (1, 2), (2, 0)].into();
        let cycle = trace_cycle_chain(3, &needs_map).unwrap();
        assert_eq!(cycle, vec![0, 1, 2]);
    }

    #[test]
    fn test_normalize_cycle() {
        // Same cycle, different rotations → same normalized form
        assert_eq!(normalize_cycle(&[0, 1, 2]), vec![0, 1, 2]);
        assert_eq!(normalize_cycle(&[1, 2, 0]), vec![0, 1, 2]);
        assert_eq!(normalize_cycle(&[2, 0, 1]), vec![0, 1, 2]);
        // Already minimal
        assert_eq!(normalize_cycle(&[3, 5, 7]), vec![3, 5, 7]);
        assert_eq!(normalize_cycle(&[5, 7, 3]), vec![3, 5, 7]);
    }

    #[test]
    fn test_format_deadlock_with_cycle() {
        let factories: Vec<Box<dyn ServiceFactory>> = vec![
            Box::new(MockFactory {
                name: "ocean::AuthService",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "ocean::UserService",
                deps: vec![],
            }),
        ];

        // AuthService needs UserService, UserService needs AuthService
        let mut errors: HashMap<usize, ServiceResolutionError> = HashMap::new();
        errors.insert(
            0,
            ServiceResolutionError::DependencyMissing {
                service: "ocean::AuthService",
                missing_type: "Service of type ocean::UserService not registered".into(),
            },
        );
        errors.insert(
            1,
            ServiceResolutionError::DependencyMissing {
                service: "ocean::UserService",
                missing_type: "Service of type ocean::AuthService not registered".into(),
            },
        );

        let constructed_set: HashSet<usize> = HashSet::new();
        let err = format_deadlock_error(&errors, &factories, &constructed_set);
        let text = err.to_string();

        assert!(text.contains("DI0001"), "should have error code: {text}");
        assert!(
            text.contains("circular runtime dependency") || text.contains("Cycle 1:"),
            "should mention cycles: {text}"
        );
        assert!(text.contains("Cycle 1:"), "should label cycle: {text}");
        assert!(
            text.contains("AuthService"),
            "should use short names: {text}"
        );
        assert!(
            text.contains("UserService"),
            "should use short names: {text}"
        );
        assert!(
            !text.contains("ocean::"),
            "should NOT show fully-qualified names: {text}"
        );
        assert!(text.contains("Lazy<T>"), "should suggest fix: {text}");
    }

    #[test]
    fn test_format_deadlock_deduplicates_cycles() {
        // 4-node cycle: A→B→C→D→A - should report exactly 1 cycle, not 4
        let factories: Vec<Box<dyn ServiceFactory>> = vec![
            Box::new(MockFactory {
                name: "svc::A",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "svc::B",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "svc::C",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "svc::D",
                deps: vec![],
            }),
        ];
        let mut errors: HashMap<usize, ServiceResolutionError> = HashMap::new();
        errors.insert(
            0,
            ServiceResolutionError::DependencyMissing {
                service: "svc::A",
                missing_type: "Service of type svc::B not registered".into(),
            },
        );
        errors.insert(
            1,
            ServiceResolutionError::DependencyMissing {
                service: "svc::B",
                missing_type: "Service of type svc::C not registered".into(),
            },
        );
        errors.insert(
            2,
            ServiceResolutionError::DependencyMissing {
                service: "svc::C",
                missing_type: "Service of type svc::D not registered".into(),
            },
        );
        errors.insert(
            3,
            ServiceResolutionError::DependencyMissing {
                service: "svc::D",
                missing_type: "Service of type svc::A not registered".into(),
            },
        );

        let constructed_set: HashSet<usize> = HashSet::new();
        let err = format_deadlock_error(&errors, &factories, &constructed_set);
        let text = err.to_string();

        assert!(text.contains("Cycle 1:"), "should have cycle 1: {text}");
        assert!(
            !text.contains("Cycle 2:"),
            "should NOT have cycle 2 (dedup): {text}"
        );
    }

    #[test]
    fn test_format_deadlock_without_cycle() {
        let factories: Vec<Box<dyn ServiceFactory>> = vec![
            Box::new(MockFactory {
                name: "ocean::AuthService",
                deps: vec![],
            }),
            Box::new(MockFactory {
                name: "ocean::UserService",
                deps: vec![],
            }),
        ];

        // AuthService needs UserService, UserService needs SomeOtherService (not a candidate)
        let mut errors: HashMap<usize, ServiceResolutionError> = HashMap::new();
        errors.insert(
            0,
            ServiceResolutionError::DependencyMissing {
                service: "ocean::AuthService",
                missing_type: "Service of type ocean::UserService not registered".into(),
            },
        );
        errors.insert(
            1,
            ServiceResolutionError::DependencyMissing {
                service: "ocean::UserService",
                missing_type: "Service of type ocean::SomeOtherService not registered".into(),
            },
        );

        let constructed_set: HashSet<usize> = HashSet::new();
        let err = format_deadlock_error(&errors, &factories, &constructed_set);
        let text = err.to_string();

        assert!(text.contains("DI0001"), "should have error code: {text}");
        assert!(
            text.contains("deadlock") || text.contains("Blocked"),
            "basic header: {text}"
        );
        assert!(text.contains("Lazy<T>"), "should suggest Lazy<T>: {text}");
        assert!(!text.contains("Cycle"), "should NOT mention cycles: {text}");
        assert!(!text.contains("ocean::"), "should use short names: {text}");
    }
}
