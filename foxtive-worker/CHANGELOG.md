# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-06-14

### Added
- **MessageProperties**: Standardized message properties for microservices metadata and distributed tracing
  - Support for content type, encoding, priority, expiration, and custom headers
  - Builder pattern for easy construction: `MessageProperties::new().with_app_id("service")`
  - Automatic extraction from RabbitMQ AMQP BasicProperties (content type, priority, headers, etc.)
  - Redis Streams field extraction as custom headers
  - Memory backend support via `enqueue_with_properties()` method
  - Comprehensive serialization/deserialization with serde
- **MessageMetadata.properties**: New optional field in MessageMetadata to store MessageProperties
- **Examples**: Complete message_properties.rs example demonstrating:
  - Custom properties with memory backend
  - Microservice identification and tracking
  - Distributed tracing with correlation IDs
  - Priority-based processing
  - TTL/expiration handling
- **Tests**: 13 comprehensive edge-case tests covering:
  - Empty properties initialization
  - Multiple headers and header overwrites
  - All standard fields (priority bounds, expiration values)
  - Serialization/deserialization round-trips
  - Unicode and empty string handling
  - Clone behavior and chaining order independence

### Changed
- Refactored MessageProperties into dedicated module (`message_properties.rs`) for better organization
- Updated all backends (RabbitMQ, Redis Streams, Memory) to populate message properties automatically
- Enhanced documentation with complete MessageProperties usage guide in README

---

## [0.2.0] - 2026-06-13

### Major Features

#### MiddlewareResult Enum for Better Control Flow
We've introduced a new `MiddlewareResult` enum that makes middleware behavior more explicit and type-safe! This is a breaking change that improves the clarity of your middleware code.

**What changed:**
- Middleware handlers now return `Result<MiddlewareResult, WorkerError>` instead of `WorkerResult<()>`
- Two clear outcomes: `Acknowledged` (middleware handled ack/nack) or `Continue` (pool should handle it)
- No more confusing `AlreadyAcknowledged` error codes — just clean, explicit control flow

**Why you'll love it:**
```rust
// Before: Unclear error-based signaling
Err(WorkerError::AlreadyAcknowledged)

// After: Crystal clear intent!
Ok(MiddlewareResult::Acknowledged)
```

This makes it immediately obvious when your middleware has already acknowledged a message, preventing those pesky double-ack bugs we all hate.

### Improvements

#### Cleaner Acknowledgment Handling
- **AckNackMiddleware** now returns `MiddlewareResult::Acknowledged` after successfully acking or nacking messages
- **WorkerPool** intelligently detects when middleware has already handled acknowledgment and skips duplicate operations
- Works seamlessly whether you use middleware or not — the pool handles both cases correctly

#### Better Type Safety
- All middleware implementations updated to use the new `MiddlewareResult` enum
- Compile-time guarantees prevent accidental double-acknowledgment
- More expressive API that documents intent through types, not just comments

### Technical Changes

**Breaking Changes:**
- `MessageHandler::handle()` signature changed:
  - Old: `async fn handle(&self, message: ReceivedMessage<T>) -> WorkerResult<()>`
  - New: `async fn handle(&self, message: ReceivedMessage<T>) -> Result<MiddlewareResult, WorkerError>`
  
- `Middleware::handle()` signature changed:
  - Old: `async fn handle(&self, message, next) -> WorkerResult<()>`
  - New: `async fn handle(&self, message, next) -> Result<MiddlewareResult, WorkerError>`

**Updated Components:**
- All built-in middleware (AckNack, Batch, CircuitBreaker, ProcessingTimeout, RetryHandler, Tracing, RateLimit)
- WorkerPool dispatch logic with proper `MiddlewareResult::Acknowledged` handling
- Test infrastructure and examples
- Documentation and docstrings

### Testing

Comprehensive test coverage ensures everything works as expected:
- **165+ tests passing** across library, integration, and doctest suites
- Tests verify correct behavior both with and without middleware
- Integration tests confirm end-to-end flows with real middleware chains
- Removed obsolete tests that relied on old `AlreadyAcknowledged` error pattern

### Migration Guide

If you have custom middleware or workers, here's how to upgrade:

**For custom middleware:**
```rust
// Before
async fn handle(&self, message, next) -> WorkerResult<()> {
    next.handle(message).await?;
    Ok(())
}

// After
async fn handle(&self, message, next) -> Result<MiddlewareResult, WorkerError> {
    let result = next.handle(message).await?;
    // Return whatever the inner handler returned
    Ok(result)
}
```

**For middleware that handles acknowledgment:**
```rust
// After acking/nacking a message
message.ack().await?;
Ok(MiddlewareResult::Acknowledged)  // Signal that ack was handled
```

**For custom test handlers:**
```rust
// Don't forget to import MiddlewareResult!
use foxtive_worker::middleware::MiddlewareResult;

#[async_trait]
impl MessageHandler for MyTestHandler {
    async fn handle(&self, _message: ReceivedMessage<serde_json::Value>) 
        -> Result<MiddlewareResult, WorkerError> {
        Ok(MiddlewareResult::Continue)
    }
}
```

### 🙏 Thanks

This release brings significant improvements to the middleware architecture, making foxtive-worker even more robust and developer-friendly. The new `MiddlewareResult` enum eliminates ambiguity and prevents common acknowledgment bugs.

---

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
