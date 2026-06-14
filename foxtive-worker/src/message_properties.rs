use serde::{Deserialize, Serialize};

/// Message properties containing backend-specific metadata.
///
/// This struct provides a standardized way to capture message properties
/// from different backends (RabbitMQ, Redis Streams, etc.) for use in
/// microservices architectures where you need to track message origin,
/// routing, and other metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MessageProperties {
    /// Content type of the message payload (e.g., "application/json")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,

    /// Content encoding (e.g., "utf-8", "gzip")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_encoding: Option<String>,

    /// Message priority (if supported by backend)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,

    /// Time-to-live or expiration duration in milliseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration: Option<u64>,

    /// Message type identifier for routing/dispatching
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_type: Option<String>,

    /// User ID associated with the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,

    /// Application ID that sent the message
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,

    /// Cluster ID (for federated messaging systems)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cluster_id: Option<String>,

    /// Reply-to address for response messages
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,

    /// Custom headers/properties specific to the backend
    /// Key-value pairs for additional metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<std::collections::HashMap<String, String>>,
}

impl MessageProperties {
    /// Create empty message properties
    pub fn new() -> Self {
        Self::default()
    }

    /// Set content type
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }

    /// Set content encoding
    pub fn with_content_encoding(mut self, encoding: impl Into<String>) -> Self {
        self.content_encoding = Some(encoding.into());
        self
    }

    /// Set message priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Set expiration/TTL in milliseconds
    pub fn with_expiration(mut self, expiration_ms: u64) -> Self {
        self.expiration = Some(expiration_ms);
        self
    }

    /// Set message type
    pub fn with_message_type(mut self, message_type: impl Into<String>) -> Self {
        self.message_type = Some(message_type.into());
        self
    }

    /// Set user ID
    pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    /// Set application ID
    pub fn with_app_id(mut self, app_id: impl Into<String>) -> Self {
        self.app_id = Some(app_id.into());
        self
    }

    /// Set cluster ID
    pub fn with_cluster_id(mut self, cluster_id: impl Into<String>) -> Self {
        self.cluster_id = Some(cluster_id.into());
        self
    }

    /// Set reply-to address
    pub fn with_reply_to(mut self, reply_to: impl Into<String>) -> Self {
        self.reply_to = Some(reply_to.into());
        self
    }

    /// Add a custom header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        if self.headers.is_none() {
            self.headers = Some(std::collections::HashMap::new());
        }
        if let Some(headers) = &mut self.headers {
            headers.insert(key.into(), value.into());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MessageMetadata;

    #[tokio::test]
    async fn test_message_properties_builder() {
        let props = MessageProperties::new()
            .with_content_type("application/json")
            .with_content_encoding("utf-8")
            .with_priority(5)
            .with_message_type("user.created")
            .with_app_id("user-service")
            .with_header("service_name", "user-service")
            .with_header("correlation_id", "abc-123");

        assert_eq!(props.content_type, Some("application/json".to_string()));
        assert_eq!(props.content_encoding, Some("utf-8".to_string()));
        assert_eq!(props.priority, Some(5));
        assert_eq!(props.message_type, Some("user.created".to_string()));
        assert_eq!(props.app_id, Some("user-service".to_string()));

        let headers = props.headers.unwrap();
        assert_eq!(
            headers.get("service_name"),
            Some(&"user-service".to_string())
        );
        assert_eq!(headers.get("correlation_id"), Some(&"abc-123".to_string()));
    }

    #[tokio::test]
    async fn test_metadata_with_properties() {
        let props = MessageProperties::new()
            .with_content_type("application/json")
            .with_app_id("test-service");

        let metadata = MessageMetadata::new("test-queue")
            .with_correlation_id("corr-456")
            .with_properties(props);

        assert_eq!(metadata.correlation_id, Some("corr-456".to_string()));
        assert!(metadata.properties.is_some());

        let props = metadata.properties.unwrap();
        assert_eq!(props.content_type, Some("application/json".to_string()));
        assert_eq!(props.app_id, Some("test-service".to_string()));
    }

    #[tokio::test]
    async fn test_empty_properties() {
        let props = MessageProperties::new();
        assert!(props.content_type.is_none());
        assert!(props.content_encoding.is_none());
        assert!(props.priority.is_none());
        assert!(props.expiration.is_none());
        assert!(props.message_type.is_none());
        assert!(props.user_id.is_none());
        assert!(props.app_id.is_none());
        assert!(props.cluster_id.is_none());
        assert!(props.reply_to.is_none());
        assert!(props.headers.is_none());
    }

    #[tokio::test]
    async fn test_multiple_headers() {
        let props = MessageProperties::new()
            .with_header("key1", "value1")
            .with_header("key2", "value2")
            .with_header("key3", "value3");

        let headers = props.headers.unwrap();
        assert_eq!(headers.len(), 3);
        assert_eq!(headers.get("key1"), Some(&"value1".to_string()));
        assert_eq!(headers.get("key2"), Some(&"value2".to_string()));
        assert_eq!(headers.get("key3"), Some(&"value3".to_string()));
    }

    #[tokio::test]
    async fn test_header_overwrite() {
        let props = MessageProperties::new()
            .with_header("key", "value1")
            .with_header("key", "value2");

        let headers = props.headers.unwrap();
        assert_eq!(headers.len(), 1);
        assert_eq!(headers.get("key"), Some(&"value2".to_string()));
    }

    #[tokio::test]
    async fn test_all_standard_fields() {
        let props = MessageProperties::new()
            .with_content_type("text/plain")
            .with_content_encoding("gzip")
            .with_priority(10)
            .with_expiration(60000)
            .with_message_type("order.created")
            .with_user_id("user-123")
            .with_app_id("order-service")
            .with_cluster_id("cluster-east")
            .with_reply_to("reply-queue");

        assert_eq!(props.content_type, Some("text/plain".to_string()));
        assert_eq!(props.content_encoding, Some("gzip".to_string()));
        assert_eq!(props.priority, Some(10));
        assert_eq!(props.expiration, Some(60000));
        assert_eq!(props.message_type, Some("order.created".to_string()));
        assert_eq!(props.user_id, Some("user-123".to_string()));
        assert_eq!(props.app_id, Some("order-service".to_string()));
        assert_eq!(props.cluster_id, Some("cluster-east".to_string()));
        assert_eq!(props.reply_to, Some("reply-queue".to_string()));
    }

    #[tokio::test]
    async fn test_serialization_deserialization() {
        let props = MessageProperties::new()
            .with_content_type("application/json")
            .with_app_id("test-service")
            .with_header("correlation_id", "abc-123");

        // Serialize to JSON
        let json = serde_json::to_string(&props).unwrap();

        // Deserialize back
        let deserialized: MessageProperties = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.content_type, props.content_type);
        assert_eq!(deserialized.app_id, props.app_id);
        assert_eq!(deserialized.headers, props.headers);
    }

    #[tokio::test]
    async fn test_serialization_skips_none_fields() {
        let props = MessageProperties::new().with_app_id("test");
        let json = serde_json::to_string(&props).unwrap();

        // Verify that None fields are not in JSON
        assert!(json.contains("app_id"));
        assert!(!json.contains("content_type"));
        assert!(!json.contains("priority"));
    }

    #[tokio::test]
    async fn test_clone_properties() {
        let props = MessageProperties::new()
            .with_content_type("application/json")
            .with_header("key", "value");

        let cloned = props.clone();
        assert_eq!(cloned.content_type, props.content_type);
        assert_eq!(cloned.headers, props.headers);
    }

    #[tokio::test]
    async fn test_priority_bounds() {
        // Test minimum priority
        let props_min = MessageProperties::new().with_priority(0);
        assert_eq!(props_min.priority, Some(0));

        // Test maximum priority (u8 max)
        let props_max = MessageProperties::new().with_priority(255);
        assert_eq!(props_max.priority, Some(255));
    }

    #[tokio::test]
    async fn test_expiration_values() {
        // Test zero expiration
        let props_zero = MessageProperties::new().with_expiration(0);
        assert_eq!(props_zero.expiration, Some(0));

        // Test large expiration (24 hours in ms)
        let props_large = MessageProperties::new().with_expiration(86400000);
        assert_eq!(props_large.expiration, Some(86400000));
    }

    #[tokio::test]
    async fn test_unicode_in_headers() {
        let props = MessageProperties::new()
            .with_header("unicode_key", "日本語")
            .with_header("emoji", "🚀");

        let headers = props.headers.unwrap();
        assert_eq!(headers.get("unicode_key"), Some(&"日本語".to_string()));
        assert_eq!(headers.get("emoji"), Some(&"🚀".to_string()));
    }

    #[tokio::test]
    async fn test_empty_strings() {
        let props = MessageProperties::new()
            .with_content_type("")
            .with_app_id("")
            .with_header("empty_key", "");

        assert_eq!(props.content_type, Some("".to_string()));
        assert_eq!(props.app_id, Some("".to_string()));

        let headers = props.headers.unwrap();
        assert_eq!(headers.get("empty_key"), Some(&"".to_string()));
    }

    #[tokio::test]
    async fn test_chaining_order_independence() {
        // Builder should work regardless of chaining order
        let props1 = MessageProperties::new()
            .with_content_type("json")
            .with_app_id("app1");

        let props2 = MessageProperties::new()
            .with_app_id("app1")
            .with_content_type("json");

        assert_eq!(props1.content_type, props2.content_type);
        assert_eq!(props1.app_id, props2.app_id);
    }
}
