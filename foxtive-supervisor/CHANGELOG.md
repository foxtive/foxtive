# Foxtive Supervisor
Foxtive changelog file

### 0.2.0 (2026-02-19)
* feat(contracts): introduce `task_id()` as the canonical task identifier; `name()` now defaults to `task_id()`
* feat(contracts): add `dependencies()` method to `SupervisedTask` — declare task IDs that must complete setup before this task starts
* feat(runtime): validate dependency graph at startup; unknown IDs and circular dependencies are caught with clear error messages before any task spawns
* feat(runtime): add prerequisite support via `add_prerequisite()` and `add_prerequisite_fn()` on `TaskRuntime`; `require()` and `require_fn()` on `Supervisor` builder
* feat(runtime): add `register_boxed()` and `register_arc()` on `TaskRuntime`; `add_boxed()` and `add_arc()` on `Supervisor` for mixed-type task registration
* feat(enums): add `SupervisionStatus::DependencyFailed` variant for tasks aborted due to a failing upstream dependency
* feat(runtime): include `task_id` field in `SupervisionResult` alongside `task_name`
* feat(error): enhance error handling with `thiserror` derive macros for structured error types
* feat(error): add comprehensive `SupervisorError` enum with detailed error variants and context
* feat(error): implement error kind classification (Configuration, Runtime, Execution, System)
* feat(error): add extension traits `SupervisorResultExt` and `SupervisorErrorExt` for ergonomic error handling
* feat(error): improve error propagation between `anyhow::Error` and `SupervisorError`
* feat(error): add utility methods for error categorization and source tracking
* chore(deps): add `thiserror = "2.0.18"` dependency for enhanced error handling
* docs(error): add comprehensive documentation with usage examples and error handling patterns

### 0.1.0 (2025-10-17)
* Initial Release
