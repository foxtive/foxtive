//! # Utility Library
//!
//! This library provides a collection of utility modules for common programming tasks.
//! Many features are optional and can be enabled via feature flags.
//!
//! ## Features
//!
//! The library supports the following optional features that can be enabled in your `Cargo.toml`:
//! - `base64`: Enables base64 encoding/decoding utilities
//! - `hmac`: Provides HMAC cryptographic functionality
//! - `jwt`: Includes JSON Web Token handling
//! - `crypto`: Enables password hashing and cryptographic functions
//! - `reqwest`: Provides HTTP client utilities
//! - `regex`: Enables regular expression functionality
//!
//! ## Modules
//!
//! ### Always Available Modules
//!
//! * `form` - Form handling utilities
//! * `fs` - File system operations
//! * `json` - JSON processing utilities
//! * `number` - Numeric type conversions and operations
//! * `once_lock` - Thread-safe initialization primitives
//! * `string` - String manipulation utilities
//! * `time` - Time and date handling functions
//! * `blk` - Re-exported tokio blocking operations
//!
//! ### Feature-Gated Modules
//!
//! * `base64` (requires `base64` feature) - Base64 encoding and decoding
//! * `hmac` (requires `hmac` feature) - HMAC message authentication
//! * `jwt` (requires `jwt` feature) - JSON Web Token operations
//! * `password` (requires `crypto` feature) - Password hashing and verification
//! * `reqwest` (requires `reqwest` feature) - HTTP client utilities
//! * `regex` (requires `regex` feature) - Regular expression operations
//!
//! ## Usage
//!
//! Add this to your `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! foxtive = { version = "0.7", features = ["base64", "jwt"] }
//! ```
//!
//! ## Examples
//!
//! Using the JSON utilities:
//! ```rust
//! use foxtive::helpers::json;
//!
//! // Example JSON operations
//! ```
//!
//! Using the file system utilities:
//! ```rust
//! use foxtive::helpers::fs;
//!
//! // Example file system operations
//! ```
//!
//! ## Feature Combinations
//!
//! Some features work well together:
//! - `jwt` + `hmac` for signed JWT tokens
//! - `crypto` + `base64` for password hashing with encoded output
//! - `reqwest` + `json` for HTTP API interactions
//!
//! ## Thread Safety
//!
//! Most utilities in this library are designed to be thread-safe and can be safely
//! used in async contexts. The `once_lock` module specifically provides thread-safe
//! initialization primitives.
//!
//! ## Error Handling
//!
//! Operations that can fail return `Result` types. It's recommended to use the
//! `anyhow` crate for error handling when working with this library.
//!
//! ## Async Support
//!
//! Many operations in this library support async/await syntax, particularly in the
//! `reqwest` and file system operations. The library uses tokio as its async runtime.
#[cfg(feature = "base64")]
pub mod base64;
pub mod form;
pub mod fs;
#[cfg(feature = "hmac")]
pub mod hmac;
pub mod json;
#[cfg(feature = "jwt")]
pub mod jwt;
pub mod number;
pub mod once_lock;
#[cfg(feature = "crypto")]
pub mod password;
#[cfg(feature = "reqwest")]
pub mod reqwest;
pub mod string;
pub mod time;
mod tokio;

pub mod env;
#[cfg(feature = "regex")]
mod regex;
mod file_ext;

pub use tokio::blk;

#[cfg(feature = "regex")]
pub use regex::*;

pub use file_ext::{FileExtHelper, COMPOUND_EXTENSIONS};
