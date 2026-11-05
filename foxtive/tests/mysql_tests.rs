//! MySQL backend integration tests.
//!
//! These tests verify MySQL-specific functionality and require the `database-mysql`
//! feature to be enabled. They also require a running MySQL 8.0+ instance.
//!
//! # Running these tests
//!
//! ```bash
//! # Set the MySQL DSN
//! export MYSQL_TEST_DSN="mysql://root:password@localhost:3306/foxtive_test"
//!
//! # Run with MySQL feature
//! cargo test --features database-mysql --test mysql_tests
//! ```

#![cfg(feature = "database-mysql")]

use foxtive::database::DbConfig;
use std::time::Duration;

#[test]
fn mysql_dsn_config_validates() {
    let config = DbConfig::create("mysql://root:password@localhost:3306/foxtive_test")
        .max_size(10)
        .min_idle(Some(2))
        .connection_timeout(Some(Duration::from_secs(5)));

    assert!(config.validate().is_ok());
}

#[test]
fn mysql_dsn_with_port_validates() {
    let config = DbConfig::create("mysql://user:pass@127.0.0.1:3307/mydb");
    assert!(config.validate().is_ok());
}

#[test]
fn mysql_dsn_with_socket_validates() {
    let config = DbConfig::create("mysql://user:pass@localhost/mydb?socket=/tmp/mysql.sock");
    assert!(config.validate().is_ok());
}

#[test]
fn mysql_config_builder_chain() {
    // Verify the builder chain works without panicking
    DbConfig::create("mysql://root:password@localhost:3306/foxtive_test")
        .max_size(20)
        .min_idle(Some(5))
        .test_on_check_out(true)
        .max_lifetime(Some(Duration::from_secs(1800)))
        .idle_timeout(Some(Duration::from_secs(600)))
        .connection_timeout(Some(Duration::from_secs(10)))
        .create_timeout(Some(Duration::from_secs(5)))
        .recycle_timeout(Some(Duration::from_secs(3)))
        .validate()
        .expect("config should be valid");
}

/// Integration test: Create a MySQL connection pool.
/// Requires `MYSQL_TEST_DSN` environment variable to be set.
#[test]
#[ignore = "requires running MySQL 8.0+ instance"]
fn create_mysql_pool() {
    let dsn = std::env::var("MYSQL_TEST_DSN").expect("MYSQL_TEST_DSN must be set");
    let config = DbConfig::create(&dsn).max_size(5);

    let pool = foxtive::database::create_db_pool(config);
    assert!(pool.is_ok(), "Failed to create MySQL pool: {:?}", pool.err());
}

/// Integration test: Create an async MySQL connection pool.
/// Requires `MYSQL_TEST_DSN` environment variable to be set.
#[test]
#[ignore = "requires running MySQL 8.0+ instance"]
fn create_async_mysql_pool() {
    let dsn = std::env::var("MYSQL_TEST_DSN").expect("MYSQL_TEST_DSN must be set");
    let config = DbConfig::create(&dsn).max_size(5);

    let pool = foxtive::database::create_async_db_pool(config);
    assert!(pool.is_ok(), "Failed to create async MySQL pool: {:?}", pool.err());
}
