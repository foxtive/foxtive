# Foxtive Framework

A modern, feature-rich Rust framework for building scalable applications with built-in support for caching, authentication, database operations, message queues, and more.

## Features

- 🔐 **JWT Authentication** - Built-in JWT token management and verification
- 💾 **Multiple Cache Drivers** - Support for Redis, Filesystem, and In-Memory caching
- 🗄️ **Database Integration** - Diesel ORM integration for database operations
- 🐰 **RabbitMQ Support** - Asynchronous message queue handling
- 🔄 **Redis Integration** - Redis connection pooling and operations
- 📝 **Template Engine** - Integrated Tera templating engine
- 🔑 **Password Hashing** - Secure password hashing using Argon2
- 🔧 **Environment Management** - Flexible environment configuration

## Installation
## Quick Start

1. Create a new Rust project:

2. Add necessary features to your `Cargo.toml`:

3. Initialize the framework:


```rust
use foxtive::setup::{FoxtiveSetup, make_state};

#[tokio::main] 
async fn main() { // Load environment variables 
    foxtive::setup::load_environment_variables("my-service");

    // Create framework setup
    let setup = FoxtiveSetup {
        env_prefix: "APP".to_string(),
        private_key: "your-private-key".to_string(),
        public_key: "your-public-key".to_string(),
        app_key: "your-app-key".to_string(),
        app_code: "your-app-code".to_string(),
        app_name: "My Foxtive App".to_string(),
        // Add other configuration based on enabled features
    };

    // Initialize the framework
    let state = make_state(setup).await;
}
```

## Running Tests

### Prerequisites
- Ensure Redis is running and accessible
- Set up required environment variables

#### Environment Variables
- `RUST_TEST_THREADS=1`: Forces tests to run sequentially instead of in parallel, which is important for tests that share resources (like Redis connections)
- `TEST_REDIS_DSN`: Connection string for the Redis instance used in testing

### Running Tests
To run the test suite, use the following command:

### Running Individual Tests
To run a specific test, you can add the test name to the command:


### Running Tests with Logging
To enable debug logging during tests, you can set the `RUST_LOG` environment variable:

```bash
RUST_LOG=debug RUST_TEST_THREADS=1 TEST_REDIS_DSN="redis://default:Pass.1234@127.0.0.1:8379" cargo test --all-features --all-targets
```
