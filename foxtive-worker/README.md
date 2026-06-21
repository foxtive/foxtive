# 🦊 Foxtive Worker

**A production-ready background worker framework for Rust.**

Process messages from RabbitMQ, Redis Streams, or any queue system with confidence. Built-in retries, circuit breakers, dead letter queues, and observability-so you can focus on your business logic.

---

## Table of Contents

- [Quick Start](#-quick-start) - Get running in 5 minutes
- [User Guide](#-user-guide) - Step-by-step learning path
  - [1. Your First Worker](#1-your-first-worker)
  - [2. Adding Reliability](#2-adding-reliability)
  - [3. Scaling Up](#3-scaling-up)
  - [4. Production Ready](#4-production-ready)
  - [5. Message Properties](#5-message-properties) - Microservices metadata & distributed tracing
  - [6. Dead Letter Queues](#6-dead-letter-queues) - Handle exhausted retries
  - [7. Smart Retry Control](#7-smart-retry-control) - Worker-controlled retry decisions
  - [8. DLQ Reprocessing](#8-dlq-reprocessing) - Requeue failed messages back to main queue
- [Examples](#-examples) - Real-world use cases
- [Configuration Reference](#configuration-reference)
  - [Resilient Backends](#making-backends-refuse-to-die) - Survive network failures
- [Troubleshooting](#troubleshooting)
- [Architecture](#architecture)

---

## Quick Start

Get a worker processing messages in under 5 minutes.

### 1. Add to Cargo.toml

```toml
[dependencies]
foxtive-worker = { version = "0.1", features = ["rabbitmq"] }
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"
serde_json = "1.0"
```

### 2. Create a Worker

```rust
use foxtive_worker::{Worker, ReceivedMessage};
use foxtive_worker::error::WorkerResult;
use async_trait::async_trait;

struct MyWorker;

#[async_trait]
impl Worker for MyWorker {
    fn id(&self) -> &str { "my-worker" }
    
    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        println!("Got message: {:?}", message.message.payload);
        // Process your message...
        Ok(())  // Return Ok(()) and let middleware handle acknowledgment
    }
}
```

**Note:** In production, you'll typically add `AckNackMiddleware` to automatically acknowledge messages based on success/failure. See [Adding Reliability](#2-adding-reliability) below.

### 3. Run It

```rust
use foxtive_worker::{WorkerPoolBuilder, backends::MemoryBackend};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // Use in-memory backend for testing
    let backend = Arc::new(MemoryBackend::new());
    let pool = WorkerPoolBuilder::new("test-pool")
        .add_worker(MyWorker)
        .build()
        .unwrap();
    
    // Send a test message
    backend.add_message(serde_json::json!({"hello": "world"})).await.unwrap();
    
    // Process it
    if let Some(msg) = backend.receive().await.unwrap() {
        pool.dispatch(msg).await.unwrap();
    }
    
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    println!("Done!");
}
```

**That's it!** You've got a working message processor. Now let's make it production-ready.

---

## User Guide

Follow this step-by-step guide to go from zero to production.

### 1. Your First Worker

Let's build something real-an email notification service.

#### The Problem
You have a queue of emails to send. Each message looks like:
```json
{
  "to": "user@example.com",
  "subject": "Welcome!",
  "body": "Thanks for signing up"
}
```

#### The Solution

```rust
use foxtive_worker::{Worker, ReceivedMessage};
use foxtive_worker::error::WorkerResult;
use async_trait::async_trait;

struct EmailWorker;

#[async_trait]
impl Worker for EmailWorker {
    fn id(&self) -> &str { 
        "email-worker" 
    }
    
    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        // Extract data from message
        let to = message.message.payload["to"]
            .as_str()
            .ok_or_else(|| WorkerError::ProcessingFailed("Missing 'to' field".into()))?;
        
        let subject = message.message.payload["subject"]
            .as_str()
            .ok_or_else(|| WorkerError::ProcessingFailed("Missing 'subject' field".into()))?;
        
        // Send the email (your implementation here)
        send_email(to, subject).await
            .map_err(|e| WorkerError::ProcessingFailed(e.to_string()))?;
        
        // Acknowledge success
        message.ack().await?;
        Ok(())
    }
}

async fn send_email(to: &str, subject: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Your SMTP logic here
    println!("Sending {} to {}", subject, to);
    Ok(())
}
```

**Key concepts:**
- **Return `Ok(())`** - Signals successful processing; middleware will ack the message
- **Return `Err(...)`** - Signals failure; middleware will nack/retry based on configuration
- **Always validate your input**-bad messages happen
- **Don't manually ack/nack** when using `AckNackMiddleware`-let it handle acknowledgment automatically

#### Try It

```rust
#[tokio::main]
async fn main() {
    let backend = Arc::new(MemoryBackend::new());
    let pool = WorkerPoolBuilder::new("email-pool")
        .add_worker(EmailWorker)
        .build()
        .unwrap();
    
    // Simulate incoming email
    backend.add_message(serde_json::json!({
        "to": "alice@example.com",
        "subject": "Hello",
        "body": "Hi there!"
    })).await.unwrap();
    
    // Process all messages
    while let Some(msg) = backend.receive().await.unwrap() {
        pool.dispatch(msg).await.unwrap();
    }
    
    tokio::time::sleep(Duration::from_millis(100)).await;
}
```

---

### 2. Adding Reliability

Your first worker works, but what happens when:
- The SMTP server is down?
- A message is malformed?
- Your worker crashes?

Let's add safety nets.

#### Automatic Retries

Messages fail sometimes. Retry them with exponential backoff:

```rust
use foxtive_worker::middleware::{RetryHandler, AckNackMiddleware};

let retry_handler = RetryHandler::default()
    .with_max_retries(3)                    // Try 3 times total
    .with_initial_backoff(Duration::from_secs(1))   // Wait 1s after first failure
    .with_max_backoff(Duration::from_secs(60));     // Never wait more than 60s

let pool = WorkerPoolBuilder::new("email-pool")
    .add_worker(EmailWorker)
    .with_middleware(AckNackMiddleware::default())  // Auto-ack/nack based on result
    .with_middleware(retry_handler)
    .build()
    .unwrap();
```

**What happens:**
1. First attempt fails → middleware nacks (requeues) → wait 1 second
2. Second attempt fails → middleware nacks (requeues) → wait 2 seconds  
3. Third attempt fails → middleware nacks (requeues) → wait 4 seconds
4. All retries exhausted → send to dead letter queue (if configured)

The backoff doubles each time (exponential) with random jitter to prevent thundering herds.

**Important:** With `AckNackMiddleware`, you don't need to call `message.ack()` or `message.nack()` in your worker-just return `Ok(())` for success or `Err(...)` for failure!

#### Dead Letter Queues

When retries are exhausted, don't lose the message-save it for later investigation:

```rust
use foxtive_worker::dlq::DeadLetterQueueBackend;

// Create a DLQ backend (in production, use Redis or file-based)
let dlq = Arc::new(DeadLetterQueueBackend::new());

let retry_handler = RetryHandler::default()
    .with_max_retries(3)
    .with_dead_letter_queue(dlq.clone());

// Later, inspect failed messages
let failed_messages = dlq.get_failed_messages().await;
for msg in failed_messages {
    eprintln!("Failed message {}: {}", msg.original_id, msg.error);
}
```

#### Circuit Breaker

If your SMTP server is down, stop hammering it:

```rust
use foxtive_worker::middleware::{CircuitBreakerMiddleware, AckNackMiddleware};

let circuit_breaker = CircuitBreakerMiddleware::new(
    5,                              // Open circuit after 5 failures
    Duration::from_secs(30)         // Try again after 30 seconds
);

let pool = WorkerPoolBuilder::new("email-pool")
    .add_worker(EmailWorker)
    .with_middleware(AckNackMiddleware::default())
    .with_middleware(circuit_breaker)
    .with_middleware(RetryHandler::default())
    .build()
    .unwrap();
```

**How it works:**
- **Closed** (normal): Messages flow through
- **Open** (after 5 failures): Reject immediately, fail fast
- **Half-Open** (after 30s): Allow one test message through
  - Success → Close circuit
  - Failure → Reopen circuit

#### Tracing

See what's happening in production:

```rust
use foxtive_worker::middleware::{TracingMiddleware, AckNackMiddleware};
use tracing_subscriber;

// Initialize tracing
tracing_subscriber::fmt::init();

let pool = WorkerPoolBuilder::new("email-pool")
    .add_worker(EmailWorker)
    .with_middleware(AckNackMiddleware::default())
    .with_middleware(TracingMiddleware::new())
    .with_middleware(RetryHandler::default())
    .build()
    .unwrap();
```

Now you get structured logs:
```
INFO Message msg-123 received from email-queue
DEBUG Processing message msg-123 (attempt 1/3)
INFO Sending Welcome! to alice@example.com
DEBUG Message msg-123 processed successfully
INFO ✓ Message msg-123 acked by middleware
```

---

### 3. Scaling Up

One worker isn't enough. Let's scale.

#### Multiple Workers

Process messages in parallel:

```rust
let pool = WorkerPoolBuilder::new("email-pool")
    .with_strategy(LoadBalancingStrategy::RoundRobin)
    .add_workers(vec![
        Arc::new(EmailWorker),
        Arc::new(EmailWorker),
        Arc::new(EmailWorker),
    ])
    .build()
    .unwrap();
```

**Load balancing strategies:**
- `RoundRobin` - Distribute evenly (worker 1, 2, 3, 1, 2, 3...)
- `Random` - Pick randomly (good enough for most cases)
- `LeastLoaded` - Send to worker with fewest active tasks (best for variable workloads)

#### Concurrency Control

Limit how many messages process simultaneously:

```rust
let pool = WorkerPoolBuilder::new("email-pool")
    .with_concurrency_limit(50)  // Max 50 concurrent messages
    .add_worker(EmailWorker)
    .build()
    .unwrap();
```

**Why limit concurrency?**
- Prevent overwhelming downstream services (SMTP servers have rate limits)
- Control memory usage
- Avoid connection pool exhaustion

**How to choose the right number:**
- CPU-bound tasks: Number of CPU cores
- I/O-bound (HTTP, DB): 50-200
- Mixed workloads: Start at 50, monitor and adjust

#### Connect to RabbitMQ

Time to use a real message broker:

```rust
use foxtive_worker::backends::RabbitMqBackend;

let config = RabbitMqConsumerConfig {
    queue_name: "emails".to_string(),
    prefetch_count: 50,  // Fetch 50 messages ahead
    ..Default::default()
};

let backend = Arc::new(
    RabbitMqBackend::new("amqp://localhost", config).await?
);

let pool = WorkerPoolBuilder::new("email-pool")
    .add_worker(EmailWorker)
    .with_middleware(RetryHandler::default())
    .build()
    .unwrap();

// Main loop
loop {
    if let Some(msg) = backend.receive().await? {
        pool.dispatch(msg).await?;
    }
}
```

**Prefetch count matters:**
- Low (1-10): Strict ordering, slower
- Medium (10-100): Good balance
- High (100+): Maximum throughput, may reorder

---

### 4. Production Ready

Final touches before deploying.

#### Graceful Shutdown

Handle SIGTERM properly so you don't lose in-flight messages:

```rust
use tokio::signal;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let backend = Arc::new(RabbitMqBackend::new("amqp://localhost", config).await?);
    let pool = WorkerPoolBuilder::new("email-pool")
        .add_worker(EmailWorker)
        .build()
        .unwrap();
    
    // Spawn consumer in background
    let backend_clone = backend.clone();
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        loop {
            if let Some(msg) = backend_clone.receive().await.unwrap() {
                pool_clone.dispatch(msg).await.unwrap();
            }
        }
    });
    
    // Wait for shutdown signal
    signal::ctrl_c().await?;
    println!("\nShutting down...");
    
    // This waits for in-flight messages to complete
    pool.shutdown().await?;
    println!("Shutdown complete");
    
    Ok(())
}
```

#### Health Checks

Expose health status for Kubernetes/orchestration:

```rust
use foxtive_worker::http::HealthEndpoint;
use axum::{Router, routing::get};

let health = HealthEndpoint::new(pool.clone());

let app = Router::new()
    .route("/health", get(|| async { health.check_health() }));

axum::Server::bind(&"0.0.0.0:8080".parse().unwrap())
    .serve(app.into_make_service())
    .await?;
```

Returns:
```json
{
  "status": "healthy",
  "pool": "email-pool",
  "workers": 3,
  "running": true
}
```

#### Metrics

Track performance in production:

```toml
foxtive-worker = { version = "0.1", features = ["metrics"] }
metrics-exporter-prometheus = "0.12"
```

```rust
use metrics_exporter_prometheus::PrometheusBuilder;

// Export metrics to Prometheus
PrometheusBuilder::new()
    .install()
    .unwrap();

// Now your pool automatically tracks:
// - foxtive_worker_messages_received_total
// - foxtive_worker_messages_processed_total
// - foxtive_worker_message_processing_duration_seconds
// - foxtive_worker_active_workers
```

Scrape at `http://localhost:9090/metrics`.

#### Rate Limiting

Don't overwhelm external services:

```rust
use foxtive_worker::middleware::RateLimitMiddleware;

// 100 messages per second, burst of 10
let rate_limiter = RateLimitMiddleware::new(100, 10);

let pool = WorkerPoolBuilder::new("email-pool")
    .add_worker(EmailWorker)
    .with_middleware(rate_limiter)
    .build()
    .unwrap();
```

Powered by the [`governor`](https://docs.rs/governor) crate-efficient, distributed-ready rate limiting.

---

### 5. Message Properties

Modern microservices architectures need rich metadata for distributed tracing, service identification, and message routing. Foxtive Worker provides standardized `MessageProperties` that work across all backends.

#### Why Message Properties?

- **Service Identification**: Track which service sent a message
- **Distributed Tracing**: Correlate requests across services
- **Priority Processing**: Handle urgent messages first
- **TTL Management**: Auto-expire stale messages
- **Custom Metadata**: Backend-specific headers and properties

#### Basic Usage

```rust
use foxtive_worker::MessageProperties;

// Create properties with builder pattern
let properties = MessageProperties::new()
    .with_content_type("application/json")
    .with_app_id("user-service")
    .with_message_type("user.created")
    .with_priority(5)
    .with_header("correlation_id", "trace-abc-123")
    .with_header("environment", "production");
```

#### Accessing Properties in Workers

```rust
#[async_trait]
impl Worker for MyWorker {
    fn id(&self) -> &str { "my-worker" }
    
    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        // Access message properties
        if let Some(props) = &message.message.metadata.properties {
            // Get standard fields
            if let Some(app_id) = &props.app_id {
                tracing::info!("Message from: {}", app_id);
            }
            
            // Get custom headers for distributed tracing
            if let Some(headers) = &props.headers {
                if let Some(correlation_id) = headers.get("correlation_id") {
                    tracing::Span::current().record("correlation_id", correlation_id);
                }
            }
            
            // Priority-based processing
            if let Some(priority) = props.priority {
                if priority >= 8 {
                    tracing::warn!("High priority message!");
                }
            }
        }
        
        // Process message...
        Ok(())
    }
}
```

#### Backend-Specific Behavior

**RabbitMQ**: Automatically extracts AMQP BasicProperties:
- Content type, encoding, priority, expiration
- User ID, app ID, reply-to
- Custom headers from FieldTable

**Redis Streams**: Extracts additional stream fields as custom headers (all fields except 'data')

**Memory Backend**: Use `enqueue_with_properties()` to set properties:

```rust
let backend = MemoryBackend::new();
backend.enqueue_with_properties(
    serde_json::json!({"key": "value"}),
    Some(properties)
);
```

#### Distributed Tracing Example

Track a request flowing through multiple services:

```rust
// Service 1: API Gateway
let props = MessageProperties::new()
    .with_app_id("api-gateway")
    .with_message_type("request.received")
    .with_header("correlation_id", "trace-xyz-789");

// Service 2: Auth Service
let props = MessageProperties::new()
    .with_app_id("auth-service")
    .with_message_type("auth.validated")
    .with_header("correlation_id", "trace-xyz-789");  // Same ID!

// Service 3: Order Service
let props = MessageProperties::new()
    .with_app_id("order-service")
    .with_message_type("order.created")
    .with_header("correlation_id", "trace-xyz-789");  // Same ID!
```

All events share the same correlation ID for end-to-end tracing.

#### Available Properties

| Field | Type | Description |
|-------|------|-------------|
| `content_type` | `Option<String>` | MIME type (e.g., "application/json") |
| `content_encoding` | `Option<String>` | Encoding (e.g., "utf-8", "gzip") |
| `priority` | `Option<u8>` | Priority level (0-255) |
| `expiration` | `Option<u64>` | TTL in milliseconds |
| `message_type` | `Option<String>` | Type identifier for routing |
| `user_id` | `Option<String>` | Associated user ID |
| `app_id` | `Option<String>` | Application/service identifier |
| `cluster_id` | `Option<String>` | Cluster ID for federated systems |
| `reply_to` | `Option<String>` | Reply address for responses |
| `headers` | `Option<HashMap>` | Custom key-value pairs |

See the [message_properties example](examples/message_properties.rs) for complete usage patterns.

---

### Acknowledgment Patterns

Foxtive Worker provides two ways to handle message acknowledgment:

#### 1. Automatic Acknowledgment (Recommended)

Use `AckNackMiddleware` to automatically ack/nack based on worker result:

```rust
use foxtive_worker::middleware::AckNackMiddleware;

let pool = WorkerPoolBuilder::new("my-pool")
    .add_worker(MyWorker)
    .with_middleware(AckNackMiddleware::default())  // Auto-ack on success, nack on failure
    .build()?;
```

**Your worker just returns results:**
```rust
async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
    do_work().await?;  // If this fails, middleware will nack
    Ok(())  // Middleware will ack
}
```

**Benefits:**
- ✅ Clean separation of concerns
- ✅ No forgotten acknowledgments
- ✅ Consistent error handling
- ✅ Works with retries, circuit breakers, etc.

#### 2. Manual Acknowledgment

If you need fine-grained control, skip `AckNackMiddleware` and ack manually:

```rust
async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
    match do_work().await {
        Ok(_) => {
            message.ack().await?;  // Explicitly acknowledge
            Ok(())
        }
        Err(e) => {
            if should_retry(&e) {
                message.nack(true).await?;  // Requeue for retry
            } else {
                message.nack(false).await?;  // Don't requeue (send to DLQ)
            }
            Err(e)
        }
    }
}
```

**When to use manual ack:**
- Conditional acknowledgment logic
- Partial processing scenarios
- Custom retry strategies per message

**⚠️ Warning:** Never mix manual ack with `AckNackMiddleware`-you'll get double-ack errors!

---

### 6. Dead Letter Queues

When messages fail all retry attempts, you don't want to lose them-you want to inspect and debug them later. That's where **Dead Letter Queues (DLQ)** come in.

#### What is a DLQ?

A DLQ is a special queue that stores messages that have exhausted all retry attempts. Instead of discarding failed messages, they're moved to the DLQ for:
- **Debugging**: Inspect why messages failed
- **Reprocessing**: Fix issues and retry manually
- **Monitoring**: Track failure rates and patterns
- **Audit trail**: Keep record of all failed processing

#### RabbitMQ DLQ Architecture

Foxtive Worker automatically sets up DLQ infrastructure when you enable delayed retries:

```
Main Queue → Retry Queue (TTL) → Main Queue → [3 attempts] → DLQ
```

**Infrastructure created automatically:**
- **Retry Queue**: `{queue_name}-retry` - Holds messages during TTL delay
- **Retry Exchange**: `{queue_name}-retry_exchange` - Routes retried messages
- **DLQ**: `{queue_name}-dlq` - Permanent storage for exhausted messages

#### Enabling DLQ

```rust
use foxtive_worker::backends::{RabbitMqBackend, RabbitMqConsumerConfig};

let config = RabbitMqConsumerConfig {
    queue_name: "email-notifications".to_string(),
    enable_delayed_retry: true,  // Enables retry queue + DLQ
    ..Default::default()
};

let backend = Arc::new(
    RabbitMqBackend::new("amqp://localhost", config).await?
);

// DLQ is automatically created as "email-notifications-dlq"
println!("DLQ name: {:?}", backend.dlq_name);
```

#### How It Works

1. **Message fails** → RetryHandler nacks with delay
2. **Published to retry queue** with TTL (e.g., 60 seconds)
3. **TTL expires** → Message dead-lettered back to main queue
4. **Attempt count preserved** via message headers (`x-retry-attempt`)
5. **After max retries** → Published to DLQ with failure metadata
6. **Original message acknowledged** (removed from main queue)

#### DLQ Message Headers

Messages in the DLQ include rich metadata in their headers:

| Header | Type | Description |
|--------|------|-------------|
| `x-original-routing-key` | String | Original message routing key |
| `x-failure-reason` | String | Error message explaining failure |
| `x-final-attempt` | Integer | Final attempt count before exhaustion |
| `x-failed-at` | String | ISO 8601 timestamp of failure |

Example DLQ message headers:
```json
{
  "x-original-routing-key": "user.created",
  "x-failure-reason": "Connection timeout after 3 retries",
  "x-final-attempt": 3,
  "x-failed-at": "2026-06-16T14:48:05.214409Z"
}
```

#### Monitoring DLQ

Check your DLQ size to detect systemic issues:

```rust
// Using RabbitMQ Management API
let dlq_messages = rabbitmq_api.get_queue_messages("email-notifications-dlq").await?;

if dlq_messages > 10 {
    tracing::warn!("High DLQ count: {} messages need attention", dlq_messages);
}
```

Or use RabbitMQ Management UI:
1. Navigate to **Queues** tab
2. Find `{your-queue}-dlq`
3. Monitor **Total** column
4. Click queue name to inspect individual messages

#### Reprocessing DLQ Messages

Manually reprocess failed messages after fixing the issue:

```rust
// Consume from DLQ
let dlq_config = RabbitMqConsumerConfig {
    queue_name: "email-notifications-dlq".to_string(),
    ..Default::default()
};

let dlq_backend = Arc::new(
    RabbitMqBackend::new("amqp://localhost", dlq_config).await?
);

// Process failed messages
while let Some(msg) = dlq_backend.receive().await? {
    println!("DLQ message: {}", msg.message.id);
    
    // Inspect failure reason from headers
    if let Some(props) = &msg.message.metadata.properties {
        if let Some(headers) = &props.headers {
            if let Some(reason) = headers.get("x-failure-reason") {
                println!("Failed because: {}", reason);
            }
        }
    }
    
    // After fixing the issue, republish to main queue
    // or handle based on your business logic
    msg.ack().await?;
}
```

#### Customizing DLQ Names

By default, DLQ names follow the pattern `{queue_name}-dlq`. You can customize this:

```rust
let config = RabbitMqConsumerConfig {
    queue_name: "emails".to_string(),
    enable_delayed_retry: true,
    retry_queue_name: Some("emails-delayed-retry".to_string()),
    // DLQ will be "emails-dlq" (auto-generated)
    ..Default::default()
};
```

#### Best Practices

✅ **Monitor DLQ growth** - Set up alerts when DLQ exceeds threshold  
✅ **Include error context** - Use descriptive error messages in workers  
✅ **Regular cleanup** - Archive or delete old DLQ messages  
✅ **Root cause analysis** - Investigate patterns in DLQ failures  
✅ **Automated reprocessing** - Build DLQ consumers for common failures  

❌ **Don't ignore DLQ** - Growing DLQ indicates systemic issues  
❌ **Don't store sensitive data** - DLQ messages persist indefinitely  
❌ **Don't disable DLQ in production** - You'll lose failed messages  

#### Example: Payment Processing with DLQ

```rust
use foxtive_worker::middleware::{RetryHandler, AckNackMiddleware};

let retry_handler = RetryHandler::default()
    .with_max_retries(3)
    .with_initial_backoff(Duration::from_secs(5));

let pool = WorkerPoolBuilder::new("payment-pool")
    .add_worker(PaymentWorker)
    .with_middleware(AckNackMiddleware::default())
    .with_middleware(retry_handler)
    .build()?;

// If payment fails 3 times:
// 1. Message moves to "payments-dlq"
// 2. Headers contain failure reason
// 3. You can inspect and reprocess manually
// 4. No payment is lost!
```

---

### 7. Smart Retry Control

Not all errors are created equal. Sometimes you want to retry, sometimes you want to give up immediately. Foxtive Worker gives you fine-grained control with `should_requeue`.

#### The Problem

By default, when a message fails, it's retried until max retries are exhausted. But what if:
- The message is malformed and will never succeed?
- It's a validation error that won't be fixed by retrying?
- The data is stale and no longer relevant?

Retrying these messages wastes resources and delays processing of valid messages.

#### The Solution: should_requeue

Implement the optional `should_requeue` method on your Worker to decide whether failed messages should be retried or sent directly to DLQ:

```rust
use foxtive_worker::{Worker, ReceivedMessage, WorkerError, RetryInfo};
use async_trait::async_trait;
use serde_json::Value;

struct OrderProcessor;

#[async_trait]
impl Worker for OrderProcessor {
    fn id(&self) -> &str { "order-processor" }
    
    async fn process(&self, message: ReceivedMessage<Value>) -> WorkerResult<()> {
        // Process order...
        Ok(())
    }
    
    // Optional: Control retry behavior
    fn should_requeue(
        &self,
        message: &ReceivedMessage<Value>,
        info: RetryInfo<'_>,
    ) -> bool {
        // Don't retry validation errors - they won't succeed
        if let WorkerError::ProcessingError(msg) = info.error {
            if msg.contains("validation failed") {
                return false; // Send to DLQ immediately
            }
        }
        
        // Don't retry malformed messages - missing required fields
        if let Some(payload) = message.message.payload.as_object() {
            if !payload.contains_key("order_id") || !payload.contains_key("amount") {
                return false;
            }
        }
        
        // Retry transient errors (network timeouts, etc.)
        true
    }
}
```

#### Common Use Cases

**1. Skip Validation Errors**
```rust
fn should_requeue(&self, _message: &ReceivedMessage<Value>, info: RetryInfo<'_>) -> bool {
    if let WorkerError::ProcessingError(msg) = info.error {
        // These errors indicate bad data that won't be fixed by retrying
        if msg.contains("invalid email") || msg.contains("missing field") {
            return false;
        }
    }
    true
}
```

**2. Check Message Age**
```rust
fn should_requeue(&self, message: &ReceivedMessage<Value>, _info: RetryInfo<'_>) -> bool {
    // Don't retry messages older than 1 hour
    if let Some(timestamp) = message.message.payload.get("created_at").and_then(|v| v.as_i64()) {
        let age = chrono::Utc::now().timestamp() - timestamp;
        if age > 3600 {
            return false; // Too old, send to DLQ
        }
    }
    true
}
```

**3. Business Logic Rules**
```rust
fn should_requeue(&self, message: &ReceivedMessage<Value>, _info: RetryInfo<'_>) -> bool {
    // Don't retry payments over $10,000 - requires manual review
    if let Some(amount) = message.message.payload.get("amount").and_then(|v| v.as_f64()) {
        if amount > 10000.0 {
            return false; // Flag for manual review
        }
    }
    true
}
```

**4. Error Type Filtering**
```rust
fn should_requeue(&self, _message: &ReceivedMessage<Value>, _info: RetryInfo<'_>) -> bool {
    match error {
        // Retry network errors - they're usually transient
        WorkerError::BackendError(_) => true,
        
        // Don't retry serialization errors - indicates bad data
        WorkerError::SerializationError(_) => false,
        
        // Default: retry
        _ => true,
    }
}
```

#### When to Use should_requeue

✅ **Use it when:**
- You can detect permanently failing messages early
- Different error types need different retry strategies
- Business logic determines retry eligibility
- You want to prevent infinite retry loops

❌ **Don't use it when:**
- All errors should be retried (default behavior is fine)
- You're using poison pill detection (automatic)
- Your errors are always transient

#### Best Practices

1. **Be conservative**: When in doubt, return `true` and retry
2. **Log decisions**: Help debugging by logging why you skip retries
3. **Check message content**: Often the payload tells you if retry is futile
4. **Consider error context**: Some errors are permanent, others transient
5. **Test thoroughly**: Make sure your logic doesn't accidentally skip valid retries

See [examples/worker_should_requeue.rs](examples/worker_should_requeue.rs) for complete working examples.

---

### 8. DLQ Reprocessing

Sometimes you fix a bug and want to reprocess all the failed messages that accumulated in your DLQ. Foxtive Worker makes this easy with `DlqManager`.

#### The Problem

Your payment service had a bug that caused 500 messages to fail. After fixing the bug, you need to:
1. Inspect each failed message in the DLQ
2. Decide which ones to retry
3. Republish them to the main queue
4. Track progress

Doing this manually is tedious and error-prone.

#### The Solution: DlqManager

`DlqManager` provides a high-level API for managing DLQ operations:

```rust
use foxtive_worker::dlq::DlqManager;
use std::sync::Arc;

// Create a DLQ manager
let manager = Arc::new(
    DlqManager::new(dlq_backend, main_backend)
);

// Reprocess all failed messages after fixing the bug
let count = manager.reprocess_all().await?;
println!("Requeued {} messages", count);
```

That's it! All messages in the DLQ are now back in the main queue for reprocessing.

#### Smart Retry Filters

Not all failed messages should be retried. Use filters to apply custom logic:

```rust
// Define a filter function
fn smart_retry_filter(msg: &DeadLetterMessage) -> bool {
    // Don't retry poison pills (messages that always fail)
    if let serde_json::Value::Object(ref ctx) = msg.failure_context {
        if let Some(poison_pill) = ctx.get("poison_pill") {
            if poison_pill.as_bool() == Some(true) {
                return false;
            }
        }
    }
    
    // Don't retry very old messages
    if let Some(failed_at) = msg.failed_at {
        let age = chrono::Utc::now() - failed_at;
        if age.num_hours() > 24 {
            return false;
        }
    }
    
    true // Retry everything else
}

// Apply the filter
let manager = Arc::new(
    DlqManager::new(dlq_backend, main_backend)
        .with_retry_filter(smart_retry_filter)
);

// Now reprocess_all will skip poison pills and old messages
let count = manager.reprocess_all().await?;
```

#### Individual Message Processing

For more control, process messages one at a time:

```rust
// Consume from DLQ
while let Some(msg) = dlq_backend.receive().await? {
    println!("Processing DLQ message: {}", msg.message.id);
    
    // Inspect failure reason
    if let Some(props) = &msg.message.metadata.properties {
        if let Some(headers) = &props.headers {
            if let Some(reason) = headers.get("x-failure-reason") {
                println!("Failed because: {}", reason);
            }
        }
    }
    
    // Decide whether to requeue
    match manager.reprocess_single(&msg).await {
        Ok(true) => println!("✓ Requeued"),
        Ok(false) => println!("⊘ Skipped (filter rejected)"),
        Err(e) => eprintln!("✗ Error: {}", e),
    }
    
    msg.ack().await?;
}
```

#### Real-World Example: Bug Fix Recovery

Here's a complete workflow for recovering from a production bug:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Connect to both DLQ and main queue
    let dlq_config = RabbitMqConsumerConfig {
        queue_name: "payments-dlq".to_string(),
        ..Default::default()
    };
    let dlq_backend = Arc::new(
        RabbitMqBackend::new("amqp://localhost", dlq_config).await?
    );
    
    let main_config = RabbitMqConsumerConfig {
        queue_name: "payments".to_string(),
        ..Default::default()
    };
    let main_backend = Arc::new(
        RabbitMqBackend::new("amqp://localhost", main_config).await?
    );
    
    // 2. Create manager with smart filtering
    let manager = Arc::new(
        DlqManager::new(dlq_backend.clone(), main_backend)
            .with_retry_filter(|msg| {
                // Skip messages that failed due to invalid data
                !is_validation_error(msg)
            })
    );
    
    // 3. Check how many messages we'll reprocess
    let dlq_size = dlq_backend.queue_size().await?;
    println!("DLQ contains {} messages", dlq_size);
    
    // 4. Reprocess all eligible messages
    println!("Starting bulk reprocessing...");
    let count = manager.reprocess_all().await?;
    println!("✓ Successfully requeued {} messages", count);
    
    // 5. Verify DLQ is now empty (or has only skipped messages)
    let remaining = dlq_backend.queue_size().await?;
    println!("{} messages remain in DLQ", remaining);
    
    Ok(())
}

fn is_validation_error(msg: &DeadLetterMessage) -> bool {
    if let serde_json::Value::Object(ref ctx) = msg.failure_context {
        if let Some(reason) = ctx.get("error") {
            return reason.as_str()
                .map(|s| s.contains("validation"))
                .unwrap_or(false);
        }
    }
    false
}
```

#### Monitoring Reprocessing

Track reprocessing progress in production:

```rust
// Spawn a monitoring task
let manager_clone = manager.clone();
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        
        let dlq_size = dlq_backend.queue_size().await.unwrap_or(0);
        let main_size = main_backend.queue_size().await.unwrap_or(0);
        
        tracing::info!(
            "Reprocessing progress: DLQ={}, Main Queue={}",
            dlq_size, main_size
        );
        
        if dlq_size == 0 {
            tracing::info!("DLQ reprocessing complete!");
            break;
        }
    }
});
```

#### When to Use DlqManager

✅ **Use it when:**
- You've fixed a bug and want to reprocess failed messages
- You need to inspect and selectively retry DLQ messages
- You want automated DLQ cleanup workflows
- You need to apply business logic to retry decisions

❌ **Don't use it when:**
- Messages should stay in DLQ for manual inspection
- You're still investigating the root cause
- The bug isn't fixed yet (you'll just create more failures)

#### Best Practices

1. **Fix the bug first**: Don't reprocess until you're confident the issue is resolved
2. **Use filters**: Not all failed messages should be retried
3. **Monitor progress**: Watch queue sizes to track reprocessing
4. **Test in staging**: Try reprocessing on a copy of production data first
5. **Keep backups**: Archive DLQ messages before reprocessing
6. **Rate limit**: If reprocessing thousands of messages, consider rate limiting

See [examples/dlq_requeue.rs](examples/dlq_requeue.rs) for complete working examples.

---

## Examples

Complete, copy-paste ready examples for common scenarios.

### Payment Processing

Process payments with timeouts and circuit breakers:

```rust
struct PaymentWorker;

#[async_trait]
impl Worker for PaymentWorker {
    fn id(&self) -> &str { "payment-worker" }
    
    fn processing_timeout(&self) -> Option<Duration> {
        Some(Duration::from_secs(30))  // Fail fast if payment hangs
    }
    
    async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        let order_id = message.message.payload["order_id"].as_str().unwrap();
        let amount = message.message.payload["amount"].as_f64().unwrap();
        
        charge_payment(order_id, amount).await?;
        Ok(())  // AckNackMiddleware will ack automatically
    }
}

let pool = WorkerPoolBuilder::new("payment-pool")
    .with_concurrency_limit(20)  // Conservative-payments are critical
    .add_worker(PaymentWorker)
    .with_middleware(AckNackMiddleware::default())
    .with_middleware(CircuitBreakerMiddleware::new(3, Duration::from_secs(60)))
    .with_middleware(RetryHandler::default().with_max_retries(2))
    .build()?;
```

### Batch Database Updates

Process updates in batches for efficiency:

```rust
use foxtive_worker::{BatchHandler, MessageBatch, BatchConfig};
use foxtive_worker::middleware::BatchMiddleware;

struct DatabaseBatchHandler;

#[async_trait]
impl BatchHandler for DatabaseBatchHandler {
    async fn process_batch(&self, batch: MessageBatch<serde_json::Value>) -> WorkerResult<()> {
        let updates: Vec<_> = batch.messages.iter()
            .map(|msg| msg.message.payload.clone())
            .collect();
        
        // Single bulk insert instead of N individual inserts
        bulk_insert(updates).await?;
        Ok(())
    }
}

let config = BatchConfig::default()
    .with_batch_size(100)              // Process 100 at a time
    .with_flush_interval(Duration::from_secs(5));  // Or every 5 seconds

let batch_middleware = BatchMiddleware::new(
    Arc::new(DatabaseBatchHandler),
    config
);

let pool = WorkerPoolBuilder::new("db-pool")
    .add_worker(DbWorker)
    .with_middleware(batch_middleware)
    .build()?;
```

### Image Processing Pipeline

Chain multiple workers together:

```rust
// Worker 1: Download image
struct DownloadWorker;

// Worker 2: Resize image  
struct ResizeWorker;

// Worker 3: Upload to CDN
struct UploadWorker;

// Separate pools for each stage
let download_pool = WorkerPoolBuilder::new("download-pool")
    .with_concurrency_limit(10)  // Network-bound
    .add_worker(DownloadWorker)
    .build()?;

let resize_pool = WorkerPoolBuilder::new("resize-pool")
    .with_concurrency_limit(4)  // CPU-bound
    .add_worker(ResizeWorker)
    .build()?;

let upload_pool = WorkerPoolBuilder::new("upload-pool")
    .with_concurrency_limit(20)  // Network-bound
    .add_worker(UploadWorker)
    .build()?;
```

Each worker publishes to the next queue in the pipeline.

---

## Configuration Reference

### RetryHandler

```rust
RetryHandler::default()
    .with_max_retries(5)                      // Total attempts
    .with_initial_backoff(Duration::from_secs(1))  // First retry delay
    .with_max_backoff(Duration::from_secs(60))     // Maximum delay
    .with_backoff_multiplier(2.0)             // Exponential factor
    .with_jitter(true)                        // Add randomness
    .with_dead_letter_queue(dlq)              // Where to send exhausted messages
    .with_poison_pill_tracker(tracker)        // Detect always-failing messages
```

### RabbitMQ Backend

```rust
RabbitMqConsumerConfig {
    queue_name: "my-queue".to_string(),
    consumer_tag: "my-consumer".to_string(),
    prefetch_count: 50,        // Messages to fetch ahead
    auto_ack: false,           // Always manual ack!
    requeue_on_nack: true,     // Put failed messages back
}
```

### Redis Streams Backend

```rust
RedisStreamConsumerConfig {
    stream_name: "my-stream".to_string(),
    group_name: "my-group".to_string(),
    consumer_name: "consumer-1".to_string(),
    block_ms: 5000,            // Wait up to 5s for messages
    count: 10,                 // Read 10 messages at once
    auto_ack: false,           // Manual ack
    dlq_stream_name: Some("my-dlq".to_string()),
}
```

---

## Making Backends "Refuse to Die"

Network failures happen. Brokers restart. Kubernetes reschedules pods. Your workers should survive all of this.

### The Problem

By default, if your RabbitMQ or Redis connection drops:
- `receive()` returns an error
- Your worker stops processing
- You need manual intervention to restart

### The Solution: ResilientBackend

Wrap any backend in `ResilientBackend` and it will **automatically retry forever** with exponential backoff:

```rust
use foxtive_worker::{ResilientBackend, ReconnectStrategy};
use std::sync::Arc;
use std::time::Duration;

#[tokio::main]
async fn main() {
    // Create your backend
    let rabbitmq = RabbitMqBackend::with_defaults("amqp://localhost")
        .await
        .unwrap();
    
    // Wrap it - now it won't die!
    let resilient = ResilientBackend::new(Arc::new(rabbitmq));
    
    // This will retry forever if connection drops
    loop {
        match resilient.receive().await {
            Ok(Some(msg)) => {
                println!("Got: {}", msg.message.id);
                msg.ack().await.unwrap();
            }
            Ok(None) => continue,  // No messages
            Err(e) => {
                // This is rarely reached - ResilientBackend retries internally
                tracing::error!("Unexpected: {}", e);
            }
        }
    }
}
```

### How It Works

1. **Detects failures** - Catches any error from the wrapped backend
2. **Retries indefinitely** - Never gives up (unless you explicitly shutdown)
3. **Exponential backoff** - Starts fast (1s), slows down (max 60s)
4. **Jitter** - Adds randomness to prevent thundering herd
5. **Observable** - Track connection state and retry attempts

### Custom Reconnection Strategies

Want more control? Configure your own strategy:

```rust
// Fast retries for critical systems
let aggressive = ReconnectStrategy::Exponential {
    initial: Duration::from_millis(200),  // Start at 200ms
    max: Duration::from_secs(10),          // Max 10s
    multiplier: 2.0,
    jitter_factor: 0.1,                    // 10% jitter
};

let resilient = ResilientBackendBuilder::new(Arc::new(redis))
    .with_strategy(aggressive)
    .build();
```

Or use fixed delays (good for testing):

```rust
let fixed = ReconnectStrategy::Fixed(Duration::from_secs(2));
let resilient = ResilientBackend::with_strategy(Arc::new(backend), fixed);
```

### Monitoring Connection Health

Track when things go wrong:

```rust
let resilient = ResilientBackend::new(Arc::new(rabbitmq));

// Spawn a monitor task
tokio::spawn({
    let resilient = resilient.clone();
    async move {
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            
            let connected = resilient.is_connected().await;
            let attempts = resilient.reconnect_attempts().await;
            let failures = resilient.consecutive_failures().await;
            
            if !connected || attempts > 0 {
                tracing::warn!(
                    "Connection issues: connected={}, attempts={}, failures={}",
                    connected, attempts, failures
                );
            }
        }
    }
});
```

### When to Use ResilientBackend

**Use it when:**
- Running in production (network failures are inevitable)
- Deployed on Kubernetes (pods get rescheduled)
- Using managed services (RDS, CloudAMQP, etc.)
- You want "set it and forget it" reliability

**Skip it when:**
- Testing locally (errors help you debug faster)
- You need immediate failure notification
- Building CLI tools (users expect fast feedback)

### Real-World Example

Here's how to make your email worker truly bulletproof:

```rust
use foxtive_worker::{WorkerPoolBuilder, ResilientBackend, middleware::RetryHandler};
use std::sync::Arc;

#[tokio::main]
async fn main() {
    // 1. Create resilient backend
    let rabbitmq = RabbitMqBackend::with_defaults("amqp://rabbitmq.prod")
        .await
        .unwrap();
    let backend = Arc::new(ResilientBackend::new(Arc::new(rabbitmq)));
    
    // 2. Add retry logic for transient failures
    let retry = RetryHandler::default()
        .with_max_retries(5)
        .with_initial_backoff(Duration::from_secs(1));
    
    // 3. Build pool with middleware
    let pool = WorkerPoolBuilder::new("email-workers")
        .add_worker(EmailWorker)
        .with_middleware(retry)
        .with_concurrency_limit(50)
        .build()
        .unwrap();
    
    // 4. Run forever - survives network outages, broker restarts, etc.
    pool.run_with_backend(backend).await.unwrap();
}
```

Now your worker will:
- Survive RabbitMQ restarts
- Handle temporary network partitions
- Retry failed messages with backoff
- Move poison pills to DLQ after 5 attempts
- Keep running for months without intervention

That's production-ready.

---

### Load Balancing Strategies

| Strategy | Best For | Notes |
|----------|----------|-------|
| RoundRobin | Uniform workloads | Even distribution |
| Random | Simple setups | Good enough usually |
| LeastLoaded | Variable workloads | Smartest routing |

---

## Troubleshooting

### Messages aren't being processed

**Check:**
1. Is your worker actually receiving messages?
   ```rust
   tracing_subscriber::fmt::init();  // Enable logs
   ```

2. Does the queue/stream name match what you're publishing to?

3. Are there permission issues on the queue?

4. Is your worker crashing silently? Wrap in try-catch:
   ```rust
   async fn process(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
       match self.do_work(&message).await {
           Ok(_) => Ok(()),  // Middleware will ack
           Err(e) => {
               eprintln!("Worker error: {}", e);
               Err(e)  // Middleware will nack
           }
       }
   }
   ```

5. **Using manual ack with AckNackMiddleware?** This causes PRECONDITION_FAILED errors! Choose one pattern only.

### Messages retrying forever

You have a poison pill (malformed message). Fix:

```rust
let retry_handler = RetryHandler::default()
    .with_max_retries(5)  // Don't retry forever!
    .with_dead_letter_queue(dlq);
```

Then inspect the DLQ to see what's failing.

### High memory usage

**Solutions:**
1. Reduce concurrency:
   ```rust
   .with_concurrency_limit(50)  // Instead of 500
   ```

2. Lower prefetch count (RabbitMQ):
   ```rust
   prefetch_count: 10  // Instead of 100
   ```

3. Check for memory leaks in your worker code

### Slow processing

**Debug steps:**
1. Profile your worker-the bottleneck is usually your code, not the framework
2. Increase concurrency if I/O-bound:
   ```rust
   .with_concurrency_limit(200)
   ```
3. Use batch processing for database operations
4. Check network latency to your broker

### Worker keeps crashing

Use Foxtive Supervisor for automatic restarts:

```rust
use foxtive_supervisor::Supervisor;

let supervisor = Supervisor::new()
    .add_child(pool.supervise())
    .start()
    .await?;
```

---

## Architecture

How it all fits together:

```
Message Broker (RabbitMQ/Redis)
         │
         ▼
   MessageBackend      ← Abstracts broker-specific code
         │
         ▼
   WorkerPool          ← Load balances across workers
         │
         ├──► Middleware Chain  ← Retry, circuit breaker, tracing
         │         │
         │         ▼
         └──► Worker           ← Your business logic
                   │
                   ▼
              Ack/Nack         ← Manual acknowledgment
```

### Design Decisions

**Manual ack/nack via middleware**
- `AckNackMiddleware` automatically acknowledges based on worker result
- You control acknowledgment behavior through middleware configuration
- No accidental message loss, explicit error handling
- Workers focus on business logic, not infrastructure concerns

**Middleware pipeline**
- Composable, reusable components
- Add/remove functionality without touching worker code
- Clean separation of concerns

**Trait-based backends**
- Swap RabbitMQ for Redis without changing worker logic
- Easy to add new brokers (Kafka, NATS, etc.)

**Semaphore-based concurrency**
- Precise control over parallelism
- Prevents resource exhaustion
- Works across all workers in the pool

**Spawn-per-message**
- Each message gets its own Tokio task
- Simple mental model
- Leverages Tokio's work-stealing scheduler

---

## Contributing

Contributions welcome! Guidelines:

1. **Run tests**: `cargo test`
2. **Format code**: `cargo fmt`
3. **Clippy clean**: `cargo clippy -- -D warnings`
4. **Update docs**: If you change public API, update docs

Open an issue first for big changes. Let's discuss before you spend weeks on a PR.

---

## License

MIT License - do whatever you want with this.

---

## Credits

Built with:
- [Tokio](https://tokio.rs/) - Async runtime
- [lapin](https://github.com/CleverCloud/lapin) - RabbitMQ client
- [redis](https://github.com/redis-rs/redis-rs) - Redis client
- [governor](https://github.com/antifuchs/governor) - Rate limiting
- [tracing](https://github.com/tokio-rs/tracing) - Structured logging

Inspired by Celery, Sidekiq, and Bull-but faster because Rust.

---

**Need help?** Open an issue on GitHub. Found a bug? Same thing. Want to chat? Also an issue.

Happy coding!

🦊
