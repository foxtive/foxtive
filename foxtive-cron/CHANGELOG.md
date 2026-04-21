# Foxtive Cron Changelog
Foxtive Cron changelog file

# Changelog

All notable changes to **foxtive-cron** will be documented in this file.

---

## [0.5.0] – 2026-04-16

### Added
* **Cron Expression Builder** - Fluent API for building cron expressions programmatically
  * Type-safe `Month` and `Weekday` enums prevent invalid values
  * Field composition methods: intervals, ranges, lists, single values
  * Common presets: `hourly()`, `daily()`, `weekly()`, `monthly()`
  * Timezone support via `with_timezone()` method
  * Blackout dates via `exclude_date()` for holidays/maintenance windows
  * Execution jitter via `with_jitter()` to prevent thundering herd problems
  * Compile-time validation of all builder configurations
* **Builder Examples** - Two new comprehensive examples:
  * `builder.rs` - Demonstrates all builder features with 10 real-world scenarios
  * `real_world.rs` - Production-ready scheduling patterns (backups, monitoring, ETL, etc.)
* **Comprehensive Test Suite** - 61 new tests covering:
  * Real-world cron expression strings (24 tests)
  * Builder API verification with expected output strings (37 tests)
  * Edge cases: leap years, DST transitions, timezone conversions
  * Performance tests: rapid succession calls, long-term scheduling
  * Validation tests: malformed expressions, boundary conditions
* **Documentation Updates** - Enhanced README with:
  * Builder API quick start guide
  * Advanced feature examples (blackout dates, jitter, timezones)
  * Common builder patterns section
  * Updated examples list with new additions

### Improved
* All existing tests continue to pass (197 tests)
* Total test coverage: 258 tests across 18 test files
* Zero breaking changes - fully backward compatible with 0.4.x

---

## [0.4.0] – 2026-04-02

* Introduced internal job registry using `HashMap<String, JobItem>` for O(1) lookup.
* Ensured synchronization between execution queue (`BinaryHeap`) and registry.
* Added job cancellation via `remove_job(id: &str)`.
* Added introspection API `list_job_ids()` for querying scheduled jobs.
* Added manual execution support with `trigger_job(id: &str)`.
* Implemented global concurrency limits using `tokio::sync::Semaphore`.
* Added per-job concurrency limits.
* Introduced execution timeouts via `tokio::time::timeout`.
* Implemented graceful shutdown for controlled scheduler termination.
* Added job priority handling for deterministic execution ordering.
* Added `MisfirePolicy` with support for:
  * `Skip`
  * `FireOnce`
  * `FireAll`
* Implemented retry strategies:
  * Fixed interval retries
  * Exponential backoff
* Introduced persistence abstraction via `JobStore` trait.
* Added `InMemoryJobStore` reference implementation.
* Added time zone support via `chrono-tz`.
* Introduced one-time job scheduling.
* Added delayed start support for recurring jobs.
* Implemented `CronBuilder` for flexible configuration.
* Replaced `anyhow::Result` with domain-specific `CronError`.
* Added event hooks for job lifecycle events:
  * Started
  * Finished
  * Failed
  * Retrying
* Integrated metrics collection:
  * Counters for executions
  * Histograms for performance tracking

### 0.3.0 (2026-02-19)
* feat(contracts): add `ValidatedSchedule` type - cron expressions are now parsed and validated eagerly at registration time, returning an error immediately on invalid input
* feat(contracts): add `id()` method to `JobContract` for stable job identity, enabling future cancellation and deduplication
* feat(contracts): `name()`, `id()`, and `description()` now return `Cow<'_, str>` instead of `String` to avoid unnecessary heap allocations
* feat(contracts): add lifecycle hooks `on_start`, `on_complete`, and `on_error` to `JobContract` - all default to no-ops
* feat(scheduler): fix job starvation bug - scheduler now peeks at the heap instead of popping, then drains all jobs due at the same tick before sleeping again
* feat(scheduler): multiple jobs scheduled at the same tick now fire concurrently in the same iteration
* feat(scheduler): emit a warning log when the cron queue is empty and the scheduler exits
* feat(job): `JobItem::run` now invokes the full lifecycle sequence (`on_start` → `run` → `on_complete` / `on_error`)
* feat(fn_job): `FnJob::new` and `FnJob::new_blocking` now return `CronResult<Self>` and accept an explicit `id` parameter
* test: add comprehensive test suite covering `ValidatedSchedule`, `FnJob`, lifecycle hooks, and scheduler integration

### 0.2.0 (2024-07-19)
* feat(env): optionally use tracing logger for more advance logging capabilities

### 0.1.0 (2024-06-22)
* Initial Release