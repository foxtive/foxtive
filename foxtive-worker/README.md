# 🦊 Foxtive Worker

**A production-ready background worker framework for Rust.**

Process messages from RabbitMQ, Redis Streams, or any queue system with confidence. Built-in retries, circuit breakers, dead letter queues, and observability—so you can focus on your business logic.

---

## Table of Contents Table of Contents

- [Quick Start](#-quick-start) - Get running in 5 minutes
- [User Guide](#-user-guide) - Step-by-step learning path
  - [1. Your First Worker](#1-your-first-worker)
  - [2. Adding Reliability](#2-adding-reliability)
  - [3. Scaling Up](#3-scaling-up)
  - [4. Production Ready](#4-production-ready)
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

Let's build something real—an email notification service.

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
- **Always validate your input**—bad messages happen
- **Don't manually ack/nack** when using `AckNackMiddleware`—let it handle acknowledgment automatically

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

**Important:** With `AckNackMiddleware`, you don't need to call `message.ack()` or `message.nack()` in your worker—just return `Ok(())` for success or `Err(...)` for failure!

#### Dead Letter Queues

When retries are exhausted, don't lose the message—save it for later investigation:

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

Powered by the [`governor`](https://docs.rs/governor) crate—efficient, distributed-ready rate limiting.

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

**⚠️ Warning:** Never mix manual ack with `AckNackMiddleware`—you'll get double-ack errors!

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
    .with_concurrency_limit(20)  // Conservative—payments are critical
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
1. Profile your worker—the bottleneck is usually your code, not the framework
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

Inspired by Celery, Sidekiq, and Bull—but faster because Rust.

---

**Need help?** Open an issue on GitHub. Found a bug? Same thing. Want to chat? Also an issue.

Happy coding!

🦊
