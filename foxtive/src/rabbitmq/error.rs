use thiserror::Error;

/// Comprehensive error types for RabbitMQ operations
#[derive(Error, Debug)]
pub enum RmqError {
    #[error("Connection error: {0}")]
    Connection(lapin::Error),

    #[error("Pool error: {0}")]
    Pool(deadpool_lapin::PoolError),

    #[error("Stream terminated unexpectedly for queue '{queue}' with tag '{tag}'")]
    StreamTerminated { queue: String, tag: String },

    #[error("Consumer stream error: {0}")]
    StreamError(#[source] Box<dyn std::error::Error + Send + Sync>),

    #[error("Operation timed out after {timeout:?}: {operation}")]
    Timeout {
        operation: String,
        timeout: std::time::Duration,
    },

    #[error("Health check failed: {reason}")]
    HealthCheckFailed { reason: String },

    #[error("Channel error (state: {state:?}): channel_id={channel_id}")]
    ChannelError { state: String, channel_id: u16 },

    #[error("Acknowledgment failed for delivery {delivery_tag}: {operation} - {source}")]
    AcknowledgmentError {
        delivery_tag: u64,
        operation: String, // "ack" or "nack"
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Publish failed to exchange '{exchange}' with routing key '{routing_key}': {source}")]
    PublishError {
        exchange: String,
        routing_key: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Declaration failed for {resource_type} '{name}': {source}")]
    DeclarationError {
        resource_type: String, // "exchange", "queue", or "binding"
        name: String,
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Reconnection failed after {attempts} attempts")]
    ReconnectionFailed { attempts: usize },

    #[error("Shutdown requested")]
    ShutdownRequested,

    #[error("Configuration error: {message}")]
    Configuration { message: String },

    #[error("Serialization error: {0}")]
    Serialization(serde_json::Error),

    #[error("UTF-8 conversion error: {0}")]
    Utf8Error(std::str::Utf8Error),

    #[error("Generic error: {0}")]
    Generic(String),

    #[error("Wrapped application error: {0}")]
    AppError(#[from] anyhow::Error),
}

impl RmqError {
    /// Create a health check failed error
    pub fn health_check_failed(reason: impl Into<String>) -> Self {
        Self::HealthCheckFailed {
            reason: reason.into(),
        }
    }

    /// Create a channel error
    pub fn channel_error(state: impl Into<String>, channel_id: u16) -> Self {
        Self::ChannelError {
            state: state.into(),
            channel_id,
        }
    }

    /// Create an acknowledgment error
    pub fn ack_error(
        delivery_tag: u64,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::AcknowledgmentError {
            delivery_tag,
            operation: "ack".to_string(),
            source: Box::new(source),
        }
    }

    /// Create a nack error
    pub fn nack_error(
        delivery_tag: u64,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::AcknowledgmentError {
            delivery_tag,
            operation: "nack".to_string(),
            source: Box::new(source),
        }
    }

    /// Create a timeout error
    pub fn timeout(operation: impl Into<String>, timeout: std::time::Duration) -> Self {
        Self::Timeout {
            operation: operation.into(),
            timeout,
        }
    }

    /// Create a stream terminated error
    pub fn stream_terminated(queue: impl Into<String>, tag: impl Into<String>) -> Self {
        Self::StreamTerminated {
            queue: queue.into(),
            tag: tag.into(),
        }
    }

    /// Create a publish error
    pub fn publish_error(
        exchange: impl Into<String>,
        routing_key: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::PublishError {
            exchange: exchange.into(),
            routing_key: routing_key.into(),
            source: Box::new(source),
        }
    }

    /// Create a declaration error
    pub fn declaration_error(
        resource_type: impl Into<String>,
        name: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::DeclarationError {
            resource_type: resource_type.into(),
            name: name.into(),
            source: Box::new(source),
        }
    }
}

/// Result type alias for RabbitMQ operations
pub type RmqResult<T> = Result<T, RmqError>;

// Manual From implementations (can only have one #[from] per enum)
impl From<lapin::Error> for RmqError {
    fn from(err: lapin::Error) -> Self {
        Self::Connection(err)
    }
}

impl From<deadpool_lapin::PoolError> for RmqError {
    fn from(err: deadpool_lapin::PoolError) -> Self {
        Self::Pool(err)
    }
}

impl From<serde_json::Error> for RmqError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err)
    }
}

impl From<std::str::Utf8Error> for RmqError {
    fn from(err: std::str::Utf8Error) -> Self {
        Self::Utf8Error(err)
    }
}

/// Helper trait to convert anyhow::Error to RmqError
pub trait IntoRmqError<T> {
    fn into_rmq(self) -> RmqResult<T>;
}

impl<T> IntoRmqError<T> for Result<T, anyhow::Error> {
    fn into_rmq(self) -> RmqResult<T> {
        self.map_err(RmqError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_error_display_timeout() {
        let err = RmqError::timeout("basic_publish", Duration::from_secs(30));
        let msg = format!("{}", err);
        assert!(msg.contains("basic_publish"));
        assert!(msg.contains("30s"));
    }

    #[test]
    fn test_error_display_stream_terminated() {
        let err = RmqError::stream_terminated("my_queue", "consumer-1");
        let msg = format!("{}", err);
        assert!(msg.contains("my_queue"));
        assert!(msg.contains("consumer-1"));
    }

    #[test]
    fn test_error_display_health_check_failed() {
        let err = RmqError::health_check_failed("connection pool exhausted");
        let msg = format!("{}", err);
        assert!(msg.contains("connection pool exhausted"));
    }

    #[test]
    fn test_error_display_channel_error() {
        let err = RmqError::channel_error("Closed", 5);
        let msg = format!("{}", err);
        assert!(msg.contains("Closed"));
        assert!(msg.contains("5"));
    }

    #[test]
    fn test_error_helper_methods() {
        // Test timeout helper
        let timeout_err = RmqError::timeout("ack", Duration::from_millis(500));
        match timeout_err {
            RmqError::Timeout { operation, timeout } => {
                assert_eq!(operation, "ack");
                assert_eq!(timeout, Duration::from_millis(500));
            }
            _ => panic!("Wrong error type"),
        }

        // Test stream terminated helper
        let stream_err = RmqError::stream_terminated("queue1", "tag1");
        match stream_err {
            RmqError::StreamTerminated { queue, tag } => {
                assert_eq!(queue, "queue1");
                assert_eq!(tag, "tag1");
            }
            _ => panic!("Wrong error type"),
        }

        // Test health check failed helper
        let health_err = RmqError::health_check_failed("test reason");
        match health_err {
            RmqError::HealthCheckFailed { reason } => {
                assert_eq!(reason, "test reason");
            }
            _ => panic!("Wrong error type"),
        }

        // Test channel error helper
        let channel_err = RmqError::channel_error("Error", 10);
        match channel_err {
            RmqError::ChannelError { state, channel_id } => {
                assert_eq!(state, "Error");
                assert_eq!(channel_id, 10);
            }
            _ => panic!("Wrong error type"),
        }
    }

    #[test]
    fn test_error_from_conversions() {
        // Test serde_json::Error conversion
        let json_err = serde_json::from_str::<String>("invalid json").unwrap_err();
        let rmq_err: RmqError = json_err.into();
        match rmq_err {
            RmqError::Serialization(_) => {} // Success
            _ => panic!("Expected Serialization error"),
        }

        // Test Utf8Error conversion
        // Use a byte sequence that's definitely invalid UTF-8
        let invalid_utf8 = vec![0xC3, 0x28]; // Invalid continuation byte
        let utf8_err = std::str::from_utf8(&invalid_utf8).unwrap_err();
        let rmq_err: RmqError = utf8_err.into();
        match rmq_err {
            RmqError::Utf8Error(_) => {} // Success
            _ => panic!("Expected Utf8Error"),
        }
    }

    #[test]
    fn test_rmqerror_to_anyhow_conversion() {
        // Verify RmqError can be converted to anyhow::Error automatically
        let rmq_err = RmqError::timeout("test_op", Duration::from_secs(5));
        
        // This should compile thanks to anyhow's blanket From implementation
        let anyhow_err: anyhow::Error = rmq_err.into();
        
        assert!(anyhow_err.to_string().contains("test_op"));
        assert!(anyhow_err.to_string().contains("5s"));
    }

    #[test]
    fn test_rmqerror_variants_convert_to_anyhow() {
        // Test that various RmqError variants convert properly
        
        // Test simple variants
        let err1 = RmqError::Generic("test error".to_string());
        let anyhow_err1: anyhow::Error = err1.into();
        assert!(!anyhow_err1.to_string().is_empty());
        assert!(anyhow_err1.downcast_ref::<RmqError>().is_some());
        
        let err2 = RmqError::ShutdownRequested;
        let anyhow_err2: anyhow::Error = err2.into();
        assert!(anyhow_err2.to_string().contains("Shutdown"));
        
        let err3 = RmqError::health_check_failed("pool exhausted");
        let anyhow_err3: anyhow::Error = err3.into();
        assert!(anyhow_err3.to_string().contains("pool exhausted"));
        
        let err4 = RmqError::channel_error("Closed", 1);
        let anyhow_err4: anyhow::Error = err4.into();
        assert!(anyhow_err4.to_string().contains("Closed"));
        
        let err5 = RmqError::stream_terminated("queue", "tag");
        let anyhow_err5: anyhow::Error = err5.into();
        assert!(anyhow_err5.to_string().contains("queue"));
        
        let err6 = RmqError::Configuration {
            message: "bad config".to_string(),
        };
        let anyhow_err6: anyhow::Error = err6.into();
        assert!(anyhow_err6.to_string().contains("bad config"));
        
        let err7 = RmqError::ReconnectionFailed { attempts: 3 };
        let anyhow_err7: anyhow::Error = err7.into();
        assert!(anyhow_err7.to_string().contains("3"));
    }

    #[test]
    fn test_question_mark_operator_conversion() {
        // Simulate a function that returns AppResult but calls RmqResult functions
        fn simulate_app_result() -> anyhow::Result<()> {
            // This simulates using ? with RmqResult in an AppResult context
            let result: RmqResult<()> = Err(RmqError::timeout("op", Duration::from_secs(1)));
            result?; // Should compile and convert automatically
            Ok(())
        }

        let err = simulate_app_result().unwrap_err();
        assert!(err.to_string().contains("timed out"));
        
        // Verify we can inspect the original error
        let rmq_err = err.downcast_ref::<RmqError>().unwrap();
        match rmq_err {
            RmqError::Timeout { operation, .. } => {
                assert_eq!(operation, "op");
            }
            _ => panic!("Expected Timeout variant"),
        }
    }

    #[test]
    fn test_nested_error_conversion() {
        // Test that nested errors preserve information through conversion
        let json_err = serde_json::from_str::<String>("invalid").unwrap_err();
        let rmq_err = RmqError::Serialization(json_err);
        let anyhow_err: anyhow::Error = rmq_err.into();
        
        // The error message should contain serialization info
        assert!(anyhow_err.to_string().contains("Serialization error"));
        
        // Can still downcast to RmqError
        let downcast = anyhow_err.downcast_ref::<RmqError>();
        assert!(downcast.is_some());
        match downcast.unwrap() {
            RmqError::Serialization(_) => {} // Success
            _ => panic!("Expected Serialization variant"),
        }
    }

    #[test]
    fn test_anyhow_to_rmqerror_preserves_type() {
        // Create an AppResult error
        let app_err: anyhow::Error = anyhow::anyhow!("application error");
        
        // Convert to RmqError using IntoRmqError trait
        let result: RmqResult<()> = Err(app_err).into_rmq();
        
        // Verify it's wrapped as AppError variant
        match result.unwrap_err() {
            RmqError::AppError(err) => {
                assert_eq!(err.to_string(), "application error");
                
                // Can downcast back to original error type if needed
                // (though in this case it's just a string)
            }
            other => panic!("Expected AppError variant, got {:?}", other),
        }
    }

    #[test]
    fn test_bidirectional_conversion() {
        // Test RmqError -> anyhow::Error -> RmqError preserves the error
        let original = RmqError::timeout("test_op", Duration::from_secs(5));
        
        // Convert to anyhow
        let anyhow_err: anyhow::Error = original.into();
        
        // Verify we can downcast back to RmqError
        let recovered = anyhow_err.downcast_ref::<RmqError>().unwrap();
        match recovered {
            RmqError::Timeout { operation, timeout } => {
                assert_eq!(operation, "test_op");
                assert_eq!(timeout, &Duration::from_secs(5));
            }
            _ => panic!("Expected Timeout variant"),
        }
    }

    #[test]
    fn test_error_debug_trait() {
        let err = RmqError::Generic("test error".to_string());
        let debug_msg = format!("{:?}", err);
        assert!(debug_msg.contains("Generic"));
        assert!(debug_msg.contains("test error"));
    }

    #[test]
    fn test_generic_error() {
        let err = RmqError::Generic("custom error message".to_string());
        let msg = format!("{}", err);
        assert_eq!(msg, "Generic error: custom error message");
    }

    #[test]
    fn test_configuration_error() {
        let err = RmqError::Configuration {
            message: "invalid config".to_string(),
        };
        let msg = format!("{}", err);
        assert!(msg.contains("invalid config"));
    }

    #[test]
    fn test_shutdown_requested() {
        let err = RmqError::ShutdownRequested;
        let msg = format!("{}", err);
        assert_eq!(msg, "Shutdown requested");
    }

    #[test]
    fn test_reconnection_failed() {
        let err = RmqError::ReconnectionFailed { attempts: 5 };
        let msg = format!("{}", err);
        assert!(msg.contains("5"));
    }
}
