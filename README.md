# Foxtive Framework

A modern, feature-rich Rust framework for building scalable applications with built-in support for caching, authentication, database operations, message queues, and more.

[![Crates.io](https://img.shields.io/crates/v/foxtive)](https://crates.io/crates/foxtive)
[![Documentation](https://docs.rs/foxtive/badge.svg)](https://docs.rs/foxtive)
[![License](https://img.shields.io/crates/l/foxtive)](https://github.com/foxtive/foxtive/blob/main/LICENSE)

## Features

- 🔐 **JWT Authentication** - Built-in JWT token management and verification
- 💾 **Multiple Cache Drivers** - Support for Redis, Filesystem, and In-Memory caching
- 🗄️ **Database Integration** - Diesel ORM integration for database operations
- 🐰 **RabbitMQ Support** - Asynchronous message queue handling
- 🔄 **Redis Integration** - Redis connection pooling and operations
- 📝 **Template Engine** - Integrated Tera templating engine
- 🔑 **Password Hashing** - Secure password hashing using Argon2
- 🔧 **Environment Management** - Flexible environment configuration
- 🌐 **HTTP Client Utilities** - Built-in reqwest utilities
- 🧮 **Utility Functions** - Comprehensive helper functions for common tasks

## Installation

Add foxtive to your `Cargo.toml`:

```toml
[dependencies]
foxtive = "0.16.1"
```

Or if you want to use specific features:

```toml
[dependencies]
foxtive = { version = "0.16.1", features = ["database", "redis", "jwt", "cache-redis"] }
```

## Quick Start

1. Create a new Rust project:

```bash
cargo new my-foxtive-app
cd my-foxtive-app
```

2. Add necessary features to your `Cargo.toml`:

```toml
[dependencies]
foxtive = { version = "0.16.1", features = ["database", "redis", "jwt", "cache-redis"] }
tokio = { version = "1", features = ["full"] }
```

3. Initialize the framework:

```rust
use foxtive::setup::{FoxtiveSetup, make_state};

#[tokio::main] 
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables 
    foxtive::setup::load_environment_variables("my-service");

    // Create framework setup
    let setup = FoxtiveSetup {
        env_prefix: "APP".to_string(),
        private_key: "your-private-key".to_string(),
        public_key: "your-public-key".to_string(),
        app_key: "your-app-key".to_string(),
        app_code: "your-app-code".to_string(),
        app_name: "My Foxtive App".to_string(),
        env: foxtive::Environment::Development,
        
        // Add other configuration based on enabled features
        #[cfg(feature = "jwt")]
        jwt_iss_public_key: "your-jwt-public-key".to_string(),
        #[cfg(feature = "jwt")]
        jwt_token_lifetime: 3600,
        
        #[cfg(feature = "database")]
        db_config: foxtive::database::DbConfig {
            dsn: std::env::var("DATABASE_URL").unwrap_or("postgresql://user:pass@localhost/db".to_string()),
            pool_max_size: 10,
        },
        
        #[cfg(feature = "redis")]
        redis_config: foxtive::redis::config::RedisConfig {
            dsn: std::env::var("REDIS_URL").unwrap_or("redis://127.0.0.1:6379".to_string()),
            pool_max_size: 10,
        },
        
        #[cfg(feature = "cache")]
        cache_driver_setup: foxtive::setup::CacheDriverSetup::Redis(|redis| {
            use foxtive::cache::drivers::RedisCacheDriver;
            std::sync::Arc::new(RedisCacheDriver::new(redis))
        }),
    };

    // Initialize the framework
    let state = make_state(setup).await?;
    
    println!("Foxtive app initialized successfully!");
    Ok(())
}
```

## Features and Modules

### Cache System

Foxtive provides a flexible caching system with multiple backend drivers:

```rust
use foxtive::cache::{Cache, drivers::FilesystemCacheDriver};
use std::sync::Arc;
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct User {
    id: u32,
    name: String,
}

#[tokio::main]
async fn main() {
    // Using filesystem cache
    let driver = Arc::new(FilesystemCacheDriver::new("./cache/"));
    let cache = foxtive::cache::Cache::new(driver);
    
    let user = User { id: 1, name: "John".to_string() };
    cache.put("user:1", &user).await.unwrap();
    
    let retrieved: Option<User> = cache.get("user:1").await.unwrap();
}
```

Available cache drivers:
- `cache-redis`: Redis-based caching
- `cache-filesystem`: Filesystem-based caching
- `cache-in-memory`: In-memory caching with DashMap

### Database Integration

Foxtive integrates with Diesel ORM for database operations:

```rust
use foxtive::database::ext::DBPoolExt;

// In your handler
let db = state.database();
let users: Vec<User> = users::table.load(&mut db.conn()?)?;
```

### Redis Operations

Built-in Redis support with connection pooling:

```rust
use foxtive::prelude::Redis;

// In your handler
let redis = state.redis();
redis.set("key", "value").await?;
let value: String = redis.get("key").await?;
```

### JWT Authentication

Built-in JWT token handling:

```rust
use foxtive::helpers::jwt::Jwt;

let jwt = Jwt::new(public_key, private_key, 3600); // 1 hour expiry
let token = jwt.encode(&user_data)?;
let decoded = jwt.decode(&token)?;
```

## Available Features

Foxtive comes with many optional features that can be enabled based on your needs:

| Feature            | Description                             |
|--------------------|-----------------------------------------|
| `database`         | Diesel ORM integration for PostgreSQL   |
| `redis`            | Redis connection pooling and operations |
| `rabbitmq`         | RabbitMQ message queue integration      |
| `jwt`              | JSON Web Token handling                 |
| `crypto`           | Password hashing with Argon2            |
| `cache`            | Generic caching interface               |
| `cache-redis`      | Redis cache driver                      |
| `cache-filesystem` | Filesystem cache driver                 |
| `cache-in-memory`  | In-memory cache driver                  |
| `templating`       | Tera templating engine                  |
| `reqwest`          | HTTP client utilities                   |
| `regex`            | Regular expression support              |
| `base64`           | Base64 encoding/decoding                |
| `hmac`             | HMAC cryptographic functions            |

## Running Tests

### Prerequisites
- Ensure Redis is running and accessible
- Set up required environment variables

#### Environment Variables
- `RUST_TEST_THREADS=1`: Forces tests to run sequentially instead of in parallel, which is important for tests that share resources (like Redis connections)
- `TEST_REDIS_DSN`: Connection string for the Redis instance used in testing

### Running Tests
To run the test suite, use the following command:

```bash
RUST_TEST_THREADS=1 TEST_REDIS_DSN="redis://default:Pass.1234@127.0.0.1:8379" cargo test --all-features --all-targets
```

### Running Individual Tests
To run a specific test, you can add the test name to the command:

```bash
RUST_TEST_THREADS=1 TEST_REDIS_DSN="redis://default:Pass.1234@127.0.0.1:8379" cargo test test_name
```

### Running Tests with Logging
To enable debug logging during tests, you can set the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug RUST_TEST_THREADS=1 TEST_REDIS_DSN="redis://default:Pass.1234@127.0.0.1:8379" cargo test --all-features --all-targets
```

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.