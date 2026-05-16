# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-06-07

### Added
- Initial core worker framework with message abstractions, worker pools, and middleware pipeline.
- `RetryHandler` middleware for message retries with exponential backoff.
- Basic metrics collection integration.
- Health monitoring for worker pools and individual workers.
- `RabbitMqBackend` for consuming messages from RabbitMQ.
- `RedisStreamBackend` for consuming messages from Redis Streams.
- `MemoryBackend` for in-memory message queues (primarily for testing).
- `RateLimitMiddleware` for controlling message processing rates.
- `CircuitBreakerMiddleware` for protecting against failing services.
- `TracingMiddleware` for basic message tracing.
- `LeastLoadedBalancer` and `RandomBalancer` strategies for worker distribution.
- `MessageMetadata` for tracking message attempts, correlation IDs, etc.
- `WorkerError` for standardized error handling.

### Changed
- **`RateLimitMiddleware`**: Replaced custom `TokenBucket` with `governor` and `async-governor` crates to eliminate `Mutex` contention and improve performance in high-throughput scenarios.
- **`CircuitBreakerMiddleware`**: Enhanced `HalfOpen` state logic to allow only a single test request to pass through, improving resilience and adhering to the classic circuit breaker pattern.
- **`RedisStreamBackend`**:
    - Implemented Dead Letter Queue (DLQ) functionality: `nack(false)` now moves messages to a configurable DLQ Redis Stream and acknowledges them from the main stream.
    - Improved deserialization error handling: `serde_json::from_str` failures now return `WorkerError::DeserializationError` instead of silently converting to a generic JSON object.
    - `RedisStreamAckHandle` now holds an `Arc<RedisStreamBackend>` to enable DLQ operations.

### Fixed
- **`RedisStreamBackend`**: `nack(false)` now correctly moves messages to a DLQ and acknowledges them from the main stream, preventing infinite retry loops for problematic messages.
- **`CircuitBreakerMiddleware`**: Corrected `HalfOpen` state behavior to allow only one test request.

### Removed
- Custom `TokenBucket` implementation in `RateLimitMiddleware`.

### Security
- No specific security fixes in this release.

## [Unreleased]

### Added

### Changed

### Fixed

### Removed

### Security
