# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-04-05

### Added

#### Dynamic Task Management
- `add_task(task: T)` method for runtime task registration.
- `remove_task(id: &str)` method for runtime task removal.
- `restart_task(id: &str)` method for manual task restart.
- Pause/resume functionality for individual tasks via control messages.
- `get_task_info(id: &str)` and `list_tasks()` for task introspection.

#### Enhanced Observability
- Internal event system with `SupervisorEvent` enum covering all lifecycle events.
- `SupervisorEventListener` trait for custom event handling.
- Event broadcasting mechanism in `TaskRuntime` via tokio broadcast channels.
- Comprehensive `tracing` instrumentation with spans and correlation IDs.
- Structured logging throughout supervision lifecycle.

#### Concurrency Control
- Global concurrency limit configuration to prevent resource exhaustion.
- Per-task concurrency limits using semaphores.
- Priority-based scheduling when resources are limited.

#### Persistence Layer
- `TaskStateStore` trait for state persistence across restarts.
- `InMemoryStateStore` reference implementation.
- `FsStateStore` for filesystem-based persistence.
- Automatic state recovery during task startup.

#### Circuit Breaker Pattern
- Circuit breaker state machine (Closed/Open/Half-Open).
- Configurable failure thresholds and recovery timeouts.
- Automatic trip on consecutive failures.
- Half-open testing for recovery detection.
- Circuit breaker events integrated into observability system.

#### Graceful Shutdown Enhancements
- Configurable shutdown timeout per task.
- Forced termination after timeout expires.
- Dependency-based shutdown ordering.
- Pre-shutdown hooks for cleanup operations.
- Shutdown progress reporting.

#### Testing Utilities
- Mock task implementations for testing.
- Fake time control for backoff testing.
- Mock prerequisites for dependency testing.
- Assertion helpers for task state verification.
- Test harness for common supervision patterns.

#### Scheduling Integration
- **Cron Scheduling**: Schedule tasks using cron expressions via `foxtive-cron` integration.
- **Initial Delays**: Configure delays before first task execution.
- **Random Jitter**: Add randomness to prevent thundering herd in distributed deployments.
- **Rate Limiting**: Enforce minimum restart intervals to prevent resource exhaustion.
- **Time Windows**: Restrict task execution to specific time periods (e.g., business hours only).

#### Task Composition & Grouping
- **Task Groups**: Atomic operations on related tasks (start/stop/restart groups together).
- **Group Health Monitoring**: Aggregate health status across task groups.
- **Supervisor Hierarchies**: Create nested supervisor trees with parent-child relationships.
- **Cascading Shutdown**: Bottom-up shutdown propagation through hierarchy.
- **Task Pools**: Load-balanced worker pools with multiple strategies:
  - RoundRobin: Distribute tasks evenly in order
  - Random: Select random workers
  - LeastLoaded: Select workers with fewest active tasks
- **Conditional Dependencies**: Environment-based dependency activation using closure predicates.
- **Complex Dependency Graphs**: Support for diamond dependencies, fan-out/fan-in patterns, and deep linear chains.

#### Hot Reload Configuration
- **Runtime Configuration Updates**: Modify restart policies and backoff strategies without restarting tasks.
- **Task Enable/Disable**: Dynamically enable or disable tasks at runtime.
- **Configuration Validation**: Validate configuration changes before applying to prevent invalid states.
- **Configuration Change Events**: Emit events when task configurations are updated for audit trails.
- **Concurrent Configuration Updates**: Thread-safe configuration updates with proper locking.

#### Distributed Coordination
- **Leader Election**: Active-passive setup using Redis-based distributed locks.
- **Distributed Locks**: Prevent duplicate task execution across multiple instances.
- **Heartbeat Mechanism**: Liveness detection for multi-instance deployments.
- **Coordination Manager**: Background tasks for automatic leader renewal and heartbeat maintenance.
- **Redis Backend**: Production-ready Redis implementation using multiplexed connections.
- **CoordinationBackend Trait**: Pluggable backend interface for custom coordination implementations.

### Changed
- Refactored `TaskRuntime` to use `HashMap` for efficient task management.
- Updated `SupervisedTask` trait with new optional methods:
  - `cron_schedule()`: Return cron expression for scheduled execution
  - `min_restart_interval()`: Enforce rate limiting
  - `execution_time_window()`: Restrict execution times
  - `group_id()`: Assign tasks to groups
  - `conditional_dependencies()`: Environment-based dependencies
  - `active_dependencies()`: Get all active dependencies (regular + conditional)
- Enhanced supervision loop with comprehensive control message handling.
- Improved error propagation for dependency failures.
- Better reliability of task shutdown and cleanup.
- Bumped version to 0.3.0 to reflect significant feature additions.

### Fixed
- Race conditions in control message handling during waits.
- Proper cleanup when tasks are stopped during cron schedule waits.
- Mutex guard scoping to avoid Send trait violations across await points.
- Health status aggregation for groups with mixed states.

### Documentation
- Comprehensive README with advanced examples for all new features.
- Testing guide with best practices and utilities documentation.
- Real-world examples: microservice orchestration, circuit breaker, graceful shutdown, panic recovery, tracing.
- New examples: distributed coordination, supervisor hierarchies, task pools, cron scheduling.

### Testing
- Added 26+ new integration tests covering new features:
  - Distributed coordination unit tests with mock backend (6 tests)
  - Hierarchy edge cases: deep nesting, shutdown order, failing tasks (3 tests)
  - Hot reload scenarios: multiple tasks, rapid changes, enable/disable (7 tests)
  - Cron scheduling edge cases: invalid expressions, frequent schedules, combined delays (6 tests)
  - Task pool stress tests: large pools, concurrent operations, failing workers (4 tests)
- All tests pass with `--all-features` flag enabled.
- Improved test coverage for real-world production scenarios.

## [0.2.0] - 2026-03-15
- Initial version with core supervision and dependency management.
