# Foxtive Cron Changelog
Foxtive Cron changelog file

### 0.3.0 (2026-02-19)
* feat(contracts): add `ValidatedSchedule` type — cron expressions are now parsed and validated eagerly at registration time, returning an error immediately on invalid input
* feat(contracts): add `id()` method to `JobContract` for stable job identity, enabling future cancellation and deduplication
* feat(contracts): `name()`, `id()`, and `description()` now return `Cow<'_, str>` instead of `String` to avoid unnecessary heap allocations
* feat(contracts): add lifecycle hooks `on_start`, `on_complete`, and `on_error` to `JobContract` — all default to no-ops
* feat(scheduler): fix job starvation bug — scheduler now peeks at the heap instead of popping, then drains all jobs due at the same tick before sleeping again
* feat(scheduler): multiple jobs scheduled at the same tick now fire concurrently in the same iteration
* feat(scheduler): emit a warning log when the cron queue is empty and the scheduler exits
* feat(job): `JobItem::run` now invokes the full lifecycle sequence (`on_start` → `run` → `on_complete` / `on_error`)
* feat(fn_job): `FnJob::new` and `FnJob::new_blocking` now return `CronResult<Self>` and accept an explicit `id` parameter
* test: add comprehensive test suite covering `ValidatedSchedule`, `FnJob`, lifecycle hooks, and scheduler integration

### 0.2.0 (2024-07-19)
* feat(env): optionally use tracing logger for more advance logging capabilities

### 0.1.0 (2024-06-22)
* Initial Release