//! Automatic `From` impls for infrastructure error types.
//!
//! These impls allow using `?` to propagate common error types into `AppResult`,
//! wrapping them as `AppMessage::Infrastructure` (500 by default).

use crate::prelude::AppMessage;

// --- Always available ---

impl From<std::io::Error> for AppMessage {
    fn from(e: std::io::Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("IO error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<serde_json::Error> for AppMessage {
    fn from(e: serde_json::Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("Serialization error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<chrono::ParseError> for AppMessage {
    fn from(e: chrono::ParseError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Chrono parse error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<std::str::Utf8Error> for AppMessage {
    fn from(e: std::str::Utf8Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("UTF-8 error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<std::string::FromUtf8Error> for AppMessage {
    fn from(e: std::string::FromUtf8Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("UTF-8 conversion error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<tokio::task::JoinError> for AppMessage {
    fn from(e: tokio::task::JoinError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Task join error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<std::env::VarError> for AppMessage {
    fn from(e: std::env::VarError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Environment variable error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<std::str::ParseBoolError> for AppMessage {
    fn from(e: std::str::ParseBoolError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Boolean parse error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<std::num::ParseIntError> for AppMessage {
    fn from(e: std::num::ParseIntError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Integer parse error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<std::num::ParseFloatError> for AppMessage {
    fn from(e: std::num::ParseFloatError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Float parse error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<std::num::TryFromIntError> for AppMessage {
    fn from(e: std::num::TryFromIntError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Integer conversion error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

impl From<uuid::Error> for AppMessage {
    fn from(e: uuid::Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("UUID parse error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

// --- Feature-gated ---

#[cfg(feature = "database")]
impl From<diesel::r2d2::PoolError> for AppMessage {
    fn from(e: diesel::r2d2::PoolError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Connection pool error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "database-async")]
impl From<diesel_async::pooled_connection::deadpool::PoolError> for AppMessage {
    fn from(e: diesel_async::pooled_connection::deadpool::PoolError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Async connection pool error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "redis")]
impl From<redis::RedisError> for AppMessage {
    fn from(e: redis::RedisError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Redis error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "redis")]
impl From<deadpool_redis::PoolError> for AppMessage {
    fn from(e: deadpool_redis::PoolError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Redis pool error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "rabbitmq")]
impl From<lapin::Error> for AppMessage {
    fn from(e: lapin::Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("RabbitMQ error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "rabbitmq")]
impl From<deadpool_lapin::PoolError> for AppMessage {
    fn from(e: deadpool_lapin::PoolError) -> Self {
        AppMessage::Infrastructure {
            message: format!("RabbitMQ pool error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "rabbitmq")]
impl From<crate::rabbitmq::RmqError> for AppMessage {
    fn from(e: crate::rabbitmq::RmqError) -> Self {
        AppMessage::Infrastructure {
            message: format!("RabbitMQ error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(any(feature = "regex", feature = "cache-filesystem", feature = "cache-in-memory"))]
impl From<fancy_regex::Error> for AppMessage {
    fn from(e: fancy_regex::Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("Regex error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "templating")]
impl From<tera::Error> for AppMessage {
    fn from(e: tera::Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("Template error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

// --- Smart conversions with semantic mapping ---

#[cfg(feature = "crypto")]
impl From<argon2::Error> for AppMessage {
    fn from(e: argon2::Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("Crypto error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "base64")]
impl From<base64::DecodeError> for AppMessage {
    fn from(e: base64::DecodeError) -> Self {
        AppMessage::Infrastructure {
            message: format!("Base64 decode error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "hmac")]
impl From<hmac::digest::InvalidLength> for AppMessage {
    fn from(e: hmac::digest::InvalidLength) -> Self {
        AppMessage::Infrastructure {
            message: format!("HMAC invalid length: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "jwt")]
impl From<jsonwebtoken::errors::Error> for AppMessage {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("JWT error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(feature = "reqwest")]
impl From<reqwest::Error> for AppMessage {
    fn from(e: reqwest::Error) -> Self {
        AppMessage::Infrastructure {
            message: format!("HTTP request error: {e}"),
            source: Some(Box::new(e)),
        }
    }
}

#[cfg(any(feature = "database", feature = "database-async"))]
impl From<diesel::result::Error> for AppMessage {
    fn from(e: diesel::result::Error) -> Self {
        match e {
            diesel::result::Error::NotFound => AppMessage::not_found("Resource not found"),
            other => AppMessage::Infrastructure {
                message: format!("Database error: {other}"),
                source: Some(Box::new(other)),
            },
        }
    }
}
