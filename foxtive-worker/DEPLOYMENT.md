# Production Deployment Guide

This guide covers everything you need to deploy foxtive-worker in production environments, including configuration tuning, monitoring setup, Kubernetes deployment, and operational best practices.

---

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Configuration Tuning](#configuration-tuning)
3. [Resource Requirements](#resource-requirements)
4. [Monitoring & Observability](#monitoring--observability)
5. [Kubernetes Deployment](#kubernetes-deployment)
6. [Health Checks & Alerting](#health-checks--alerting)
7. [Scaling Strategies](#scaling-strategies)
8. [Troubleshooting](#troubleshooting)

---

## Architecture Overview

Foxtive Worker consists of several key components:

```
┌─────────────────────────────────────────────────┐
│                  Worker Pool                      │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐      │
│  │ Worker 1 │  │ Worker 2 │  │ Worker N │      │
│  └──────────┘  └──────────┘  └──────────┘      │
│         ↓             ↓             ↓            │
│  ┌─────────────────────────────────────┐        │
│  │     Middleware Pipeline              │        │
│  │  (Retry → Circuit Breaker → Trace)  │        │
│  └─────────────────────────────────────┘        │
└─────────────────────────────────────────────────┘
                    ↓
┌─────────────────────────────────────────────────┐
│              Message Backend                     │
│  (RabbitMQ / Redis Streams / Memory)            │
└─────────────────────────────────────────────────┘
```

**Key Components:**
- **WorkerPool**: Manages concurrent message processing with load balancing
- **Workers**: Individual message processors implementing the `Worker` trait
- **Middleware**: Composable processing pipeline (retry, circuit breaker, tracing)
- **Backend**: Message source (RabbitMQ, Redis Streams, or in-memory)

---

## Configuration Tuning

### Concurrency Limits

The most critical setting for performance and stability:

```rust
use foxtive_worker::pool::WorkerPool;
use foxtive_worker::strategies::LoadBalancingStrategy;

// Recommended concurrency limits by use case:
let pool = WorkerPool::with_concurrency(
    "my-pool",
    LoadBalancingStrategy::RoundRobin,
    concurrency_limit, // ← This value
    Arc::new(NoOpMetrics),
);
```

**Guidelines:**

| Use Case | Concurrency Limit | Rationale |
|----------|------------------|-----------|
| CPU-bound tasks | 2-4 × CPU cores | Avoid context switching overhead |
| I/O-bound tasks (DB queries) | 50-200 | Maximize parallel I/O operations |
| I/O-bound tasks (HTTP APIs) | 100-500 | Handle network latency efficiently |
| Mixed workload | 50-100 | Balance between CPU and I/O |
| Memory-constrained | 10-20 | Prevent OOM from too many in-flight messages |

**Example Calculation:**
```rust
// For an 8-core server processing HTTP requests:
let cpu_cores = 8;
let concurrency_limit = cpu_cores * 20; // 160 concurrent tasks
```

**Warning Signs:**
- Too low: Low throughput, workers idle
- Too high: High memory usage, thread contention, degraded performance

### Prefetch Count (RabbitMQ)

Controls how many unacknowledged messages RabbitMQ delivers:

```rust
use foxtive_worker::backends::rabbitmq::RabbitMqConsumerConfig;

let config = RabbitMqConsumerConfig {
    prefetch_count: 10, // ← Tune this
    ..Default::default()
};
```

**Guidelines:**

| Scenario | Prefetch Count | Notes |
|----------|---------------|-------|
| Fast processing (<10ms) | 50-100 | Keep pipeline full |
| Medium processing (10-100ms) | 10-20 | Balance throughput/memory |
| Slow processing (>100ms) | 5-10 | Avoid overwhelming workers |
| Large messages (>1MB) | 1-5 | Minimize memory usage |

**Dynamic Adjustment:**
```rust
// Adjust at runtime based on performance
backend.adjust_prefetch(50).await?;
```

### Message Size Considerations

Memory usage scales with message size and concurrency:

```
Memory ≈ (concurrency × avg_message_size) + worker_overhead
```

**Examples:**
- 100 concurrent × 1KB messages = ~100KB + overhead
- 100 concurrent × 1MB messages = ~100MB + overhead

**Recommendation:**
- For large messages (>100KB), reduce concurrency limit
- Use streaming/chunking for very large payloads

---

## Resource Requirements

### Memory Usage

**Baseline Memory:**
- Library overhead: ~20MB
- Per worker: ~1MB
- Per in-flight message: message_size bytes

**Formula:**
```
Total Memory ≈ 20MB + (workers × 1MB) + (concurrency × avg_message_size)
```

**Example Calculations:**

| Scenario | Workers | Concurrency | Avg Msg Size | Estimated Memory |
|----------|---------|-------------|--------------|------------------|
| Small messages | 10 | 100 | 1KB | ~210MB |
| Medium messages | 10 | 100 | 10KB | ~300MB |
| Large messages | 10 | 50 | 100KB | ~520MB |

**Memory Limits:**
- Set container memory limit to 1.5× estimated usage
- Monitor actual usage and adjust concurrency accordingly

### CPU Usage

**Typical Patterns:**
- Idle: <5% CPU
- Processing: 20-80% CPU (depends on workload)
- Shutdown spike: Brief 100% during task cancellation

**Optimization Tips:**
- Pin workers to specific CPU cores for CPU-bound tasks
- Use `taskset` or Kubernetes CPU affinity
- Monitor with `top` or Prometheus

### Disk I/O

Minimal disk usage unless:
- Logging to file (configure log rotation)
- Using persistent metrics storage
- Writing crash dumps

**Recommendation:**
- Use structured logging (JSON) for better parsing
- Rotate logs daily or at 100MB
- Store logs on separate volume from application

---

## Monitoring & Observability

### Metrics Collection

Foxtive Worker provides built-in metrics via the `WorkerMetrics` trait:

```rust
use foxtive_worker::metrics::{WorkerMetrics, NoOpMetrics};

// Implement custom metrics collector
struct PrometheusMetrics;

impl WorkerMetrics for PrometheusMetrics {
    fn record_message_processed(&self, worker_id: &str, queue: &str, start: Instant) {
        let duration = start.elapsed();
        MESSAGE_PROCESSING_DURATION
            .with_label_values(&[worker_id, queue])
            .observe(duration.as_secs_f64());
    }
    
    fn record_message_failed(&self, worker_id: &str, queue: &str, error: &str, start: Instant) {
        MESSAGE_FAILURES
            .with_label_values(&[worker_id, queue, error])
            .inc();
    }
    
    // ... implement other methods
}
```

**Key Metrics to Track:**

1. **Throughput**
   - Messages processed per second
   - Messages failed per second
   - Retry rate

2. **Latency**
   - Average processing time
   - P95/P99 processing time
   - Queue wait time

3. **Resource Usage**
   - In-flight message count
   - Worker pool saturation (%)
   - Memory usage

4. **Reliability**
   - Error rate (%)
   - Circuit breaker state
   - Dead letter queue size

### Tracing Integration

Enable distributed tracing with the `tracing` feature:

```rust
use foxtive_worker::middleware::TracingMiddleware;

let pool = WorkerPoolBuilder::new("my-pool")
    .add_worker(my_worker)
    .with_middlewares(vec![
        Arc::new(TracingMiddleware::new()),
    ])
    .build()?;
```

**Trace Spans Include:**
- Message receipt
- Processing duration
- Ack/nack operations
- Retry attempts
- Error details

**Export to Jaeger/Zipkin:**
```toml
# Cargo.toml
[dependencies]
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-opentelemetry = "0.20"
opentelemetry = { version = "0.20", features = ["rt-tokio"] }
opentelemetry-jaeger = { version = "0.19", features = ["rt-tokio"] }
```

```rust
use tracing_subscriber::layer::SubscriberExt;
use tracing_opentelemetry::OpenTelemetryLayer;
use opentelemetry_jaeger::JaegerPipeline;

fn init_tracing() {
    let tracer = JaegerPipeline::new()
        .with_service_name("foxtive-worker")
        .install_simple()
        .unwrap();
    
    let subscriber = tracing_subscriber::registry()
        .with(OpenTelemetryLayer::new(tracer))
        .with(tracing_subscriber::EnvFilter::from_default_env());
    
    tracing::subscriber::set_global_default(subscriber).unwrap();
}
```

### Logging Best Practices

**Structured Logging Format:**
```rust
use tracing_subscriber::fmt;

fmt()
    .json()  // JSON format for easy parsing
    .with_target(false)
    .with_thread_ids(true)
    .with_level(true)
    .init();
```

**Log Levels:**
- `ERROR`: Message processing failures, backend errors
- `WARN`: Retries, circuit breaker trips, high load
- `INFO`: Worker startup/shutdown, pool status
- `DEBUG`: Individual message processing details
- `TRACE`: Detailed internal operations

**Example Log Output:**
```json
{
  "timestamp": "2026-06-11T10:30:00Z",
  "level": "INFO",
  "message": "Message msg-123 processed successfully",
  "worker_id": "worker-5",
  "queue": "orders",
  "duration_ms": 45
}
```

---

## Kubernetes Deployment

### Basic Deployment Manifest

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: foxtive-worker
  labels:
    app: foxtive-worker
spec:
  replicas: 3
  selector:
    matchLabels:
      app: foxtive-worker
  template:
    metadata:
      labels:
        app: foxtive-worker
    spec:
      containers:
      - name: worker
        image: your-registry/foxtive-worker:latest
        resources:
          requests:
            memory: "256Mi"
            cpu: "500m"
          limits:
            memory: "512Mi"
            cpu: "1000m"
        env:
        - name: RUST_LOG
          value: "info"
        - name: WORKER_CONCURRENCY
          value: "100"
        - name: RABBITMQ_URL
          valueFrom:
            secretKeyRef:
              name: rabbitmq-secret
              key: url
        ports:
        - containerPort: 8080
          name: health
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 30
          timeoutSeconds: 5
          failureThreshold: 3
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 10
          timeoutSeconds: 3
          failureThreshold: 3
```

### Horizontal Pod Autoscaler

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: foxtive-worker-hpa
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: foxtive-worker
  minReplicas: 2
  maxReplicas: 10
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Resource
    resource:
      name: memory
      target:
        type: Utilization
        averageUtilization: 80
```

### ConfigMap for Configuration

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: foxtive-worker-config
data:
  config.toml: |
    [worker]
    concurrency = 100
    strategy = "RoundRobin"
    
    [rabbitmq]
    prefetch_count = 20
    queue_name = "production_queue"
    
    [logging]
    level = "info"
    format = "json"
```

Mount in deployment:
```yaml
volumeMounts:
- name: config
  mountPath: /etc/foxtive-worker
volumes:
- name: config
  configMap:
    name: foxtive-worker-config
```

### Service Mesh Integration (Istio)

```yaml
apiVersion: networking.istio.io/v1beta1
kind: VirtualService
metadata:
  name: foxtive-worker
spec:
  hosts:
  - foxtive-worker
  http:
  - route:
    - destination:
        host: foxtive-worker
        port:
          number: 8080
    timeout: 30s
    retries:
      attempts: 3
      perTryTimeout: 10s
```

---

## Health Checks & Alerting

### Health Check Endpoints

Foxtive Worker provides HTTP health endpoints:

```rust
use foxtive_worker::http::HealthServer;

let health_server = HealthServer::new(pool.clone(), "0.0.0.0:8080");
tokio::spawn(health_server.start());
```

**Endpoints:**

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Liveness probe - is the service running? |
| `/ready` | GET | Readiness probe - can it accept traffic? |
| `/metrics` | GET | Prometheus metrics export |

**Response Examples:**

Healthy:
```json
{
  "status": "healthy",
  "pool": {
    "name": "my-pool",
    "workers": 10,
    "in_flight": 45,
    "saturation": 0.45
  }
}
```

Degraded:
```json
{
  "status": "degraded",
  "reason": "Pool near capacity: 950 in-flight messages (95% saturation)",
  "pool": {
    "name": "my-pool",
    "workers": 10,
    "in_flight": 950,
    "saturation": 0.95
  }
}
```

### Prometheus Alerting Rules

```yaml
groups:
- name: foxtive-worker-alerts
  rules:
  
  # High error rate
  - alert: HighErrorRate
    expr: rate(foxtive_worker_message_failures_total[5m]) > 0.1
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "High error rate detected"
      description: "Error rate is {{ $value }} errors/sec"
  
  # Pool saturation
  - alert: PoolSaturation
    expr: foxtive_worker_pool_saturation > 0.9
    for: 2m
    labels:
      severity: warning
    annotations:
      summary: "Worker pool near capacity"
      description: "Pool saturation is {{ $value | humanizePercentage }}"
  
  # High latency
  - alert: HighProcessingLatency
    expr: histogram_quantile(0.95, rate(foxtive_worker_processing_duration_seconds_bucket[5m])) > 1.0
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "High processing latency"
      description: "P95 latency is {{ $value }}s"
  
  # Circuit breaker open
  - alert: CircuitBreakerOpen
    expr: foxtive_worker_circuit_breaker_state == 1
    for: 1m
    labels:
      severity: critical
    annotations:
      summary: "Circuit breaker is open"
      description: "Circuit breaker tripped for {{ $labels.worker_id }}"
  
  # Dead letter queue growth
  - alert: DLQGrowing
    expr: rate(foxtive_worker_dlq_messages_total[15m]) > 0
    for: 15m
    labels:
      severity: warning
    annotations:
      summary: "Dead letter queue growing"
      description: "{{ $value }} messages/min sent to DLQ"
```

### Grafana Dashboard

Create a dashboard with these panels:

1. **Throughput**
   - Query: `rate(foxtive_worker_messages_processed_total[5m])`
   - Type: Time series

2. **Error Rate**
   - Query: `rate(foxtive_worker_message_failures_total[5m])`
   - Type: Time series

3. **Latency Percentiles**
   - Query: `histogram_quantile(0.95, rate(foxtive_worker_processing_duration_seconds_bucket[5m]))`
   - Type: Time series

4. **Pool Saturation**
   - Query: `foxtive_worker_pool_saturation`
   - Type: Gauge with threshold at 0.9

5. **In-Flight Messages**
   - Query: `foxtive_worker_in_flight_messages`
   - Type: Time series

6. **Circuit Breaker State**
   - Query: `foxtive_worker_circuit_breaker_state`
   - Type: Status map (0=closed, 1=open, 2=half-open)

---

## Scaling Strategies

### Vertical Scaling (Increase Resources)

**When to Scale Up:**
- Single instance hitting CPU/memory limits
- Cannot add more instances due to queue partitioning
- Need lower latency (fewer network hops)

**How to Scale:**
```yaml
# Increase resource limits
resources:
  limits:
    memory: "1Gi"  # Was 512Mi
    cpu: "2000m"   # Was 1000m
```

**Adjust concurrency:**
```rust
// Increase concurrency with more resources
let pool = WorkerPool::with_concurrency(
    "my-pool",
    strategy,
    200,  // Was 100
    metrics,
);
```

### Horizontal Scaling (Add Instances)

**When to Scale Out:**
- Need higher throughput
- Want better fault tolerance
- Geographic distribution needed

**How to Scale:**
```bash
kubectl scale deployment foxtive-worker --replicas=5
```

**Considerations:**
- Each instance needs unique consumer tag (RabbitMQ)
- Each instance needs unique consumer name (Redis Streams)
- Use environment variables or pod name for uniqueness

```rust
let consumer_name = format!("consumer-{}", std::env::var("POD_NAME").unwrap_or_default());
```

### Auto-Scaling

**Based on Queue Depth:**
```yaml
# KEDA scaler example for RabbitMQ
apiVersion: keda.sh/v1alpha1
kind: ScaledObject
metadata:
  name: foxtive-worker-scaler
spec:
  scaleTargetRef:
    name: foxtive-worker
  triggers:
  - type: rabbitmq
    metadata:
      queueName: production_queue
      queueLength: "100"  # Scale when 100 messages queued
      host: rabbitmq-secret
```

**Based on CPU/Memory:**
```yaml
# HPA example (shown earlier)
# Scales based on resource utilization
```

---

## Troubleshooting

### Common Issues

#### 1. High Memory Usage

**Symptoms:**
- OOM kills
- Slow garbage collection
- Swap usage increasing

**Diagnosis:**
```bash
# Check memory usage
kubectl top pods -l app=foxtive-worker

# Inspect in-flight messages
curl http://pod-ip:8080/health | jq '.pool.in_flight'
```

**Solutions:**
- Reduce concurrency limit
- Decrease prefetch count
- Process larger messages in batches
- Add memory limits and monitor

#### 2. Low Throughput

**Symptoms:**
- Messages accumulating in queue
- Workers mostly idle
- Low CPU usage

**Diagnosis:**
```bash
# Check worker utilization
curl http://pod-ip:8080/metrics | grep foxtive_worker_pool_saturation

# Check processing times
curl http://pod-ip:8080/metrics | grep foxtive_worker_processing_duration
```

**Solutions:**
- Increase concurrency limit
- Increase prefetch count (RabbitMQ)
- Optimize worker processing logic
- Add more worker instances

#### 3. High Error Rate

**Symptoms:**
- Many messages in dead letter queue
- Circuit breaker frequently opening
- Retry exhaustion

**Diagnosis:**
```bash
# Check error types
curl http://pod-ip:8080/metrics | grep foxtive_worker_message_failures

# Check circuit breaker state
curl http://pod-ip:8080/metrics | grep foxtive_worker_circuit_breaker
```

**Solutions:**
- Fix underlying worker bugs
- Adjust retry configuration
- Increase circuit breaker thresholds
- Investigate backend connectivity

#### 4. Shutdown Hanging

**Symptoms:**
- Pod termination taking >30 seconds
- Kubernetes force-killing pods
- Messages being reprocessed

**Diagnosis:**
```bash
# Check shutdown logs
kubectl logs pod-name --previous | grep "shutdown"

# Check in-flight count during shutdown
curl http://pod-ip:8080/health
```

**Solutions:**
- Ensure graceful shutdown is called
- Reduce shutdown timeout if acceptable
- Investigate long-running tasks
- Add task timeouts via middleware

```rust
use foxtive_worker::middleware::ProcessingTimeoutMiddleware;

let pool = WorkerPoolBuilder::new("my-pool")
    .add_worker(my_worker)
    .with_middlewares(vec![
        Arc::new(ProcessingTimeoutMiddleware::new(Duration::from_secs(30))),
    ])
    .build()?;
```

### Debugging Tools

#### Enable Debug Logging

```bash
export RUST_LOG=debug
# or
export RUST_LOG=foxtive_worker=debug
```

#### Trace Specific Messages

```rust
use tracing::Span;

async fn process(&self, message: ReceivedMessage<Value>) -> WorkerResult<()> {
    let span = Span::current();
    span.record("message_id", &message.message.id);
    
    // Processing logic...
}
```

#### Profile Performance

```toml
# Cargo.toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
```

Run benchmarks:
```bash
cargo bench --features memory
```

### Support Resources

- **Documentation**: See README.md for user guide
- **Examples**: Check `examples/` directory for common patterns
- **Issues**: Report bugs on GitHub
- **Community**: Join our Discord/Slack channel

---

## Quick Reference

### Default Values

| Setting | Default | Recommended Production |
|---------|---------|----------------------|
| Concurrency | 1000 | 50-200 (tune per workload) |
| Prefetch (RabbitMQ) | 10 | 10-50 (tune per message size) |
| Shutdown Timeout | 30s | 30-60s |
| Retry Attempts | 3 | 3-5 |
| Circuit Breaker Threshold | 5 failures | 5-10 failures |

### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `RUST_LOG` | Log level | `info` or `debug` |
| `WORKER_CONCURRENCY` | Pool concurrency | `100` |
| `RABBITMQ_URL` | RabbitMQ connection | `amqp://localhost:5672` |
| `REDIS_URL` | Redis connection | `redis://localhost:6379` |
| `PORT` | Health server port | `8080` |

### Health Check URLs

| URL | Purpose | Expected Response |
|-----|---------|-------------------|
| `GET /health` | Liveness | 200 OK or 503 Unhealthy |
| `GET /ready` | Readiness | 200 OK or 503 Not Ready |
| `GET /metrics` | Prometheus | Text format metrics |

---

**Last Updated:** June 11, 2026  
**Version:** foxtive-worker 0.1.0  

🦊
