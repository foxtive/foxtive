use async_trait::async_trait;
use std::sync::Arc;

use crate::error::WorkerResult;
use crate::message::ReceivedMessage;

pub mod ack_nack;
pub mod batch;
pub mod circuit_breaker;
pub mod processing_timeout;
#[cfg(feature = "rate-limit")]
pub mod rate_limit;
pub mod retry_handler;
pub mod tracing;

/// A message handler that processes messages.
///
/// This trait represents a single step in the middleware chain.
/// Handlers can be middleware components or final workers.
#[async_trait]
pub trait MessageHandler: Send + Sync {
    /// Process a message.
    ///
    /// # Arguments
    /// * `message` - The message to process
    ///
    /// # Returns
    /// * `Ok(())` if processing succeeded
    /// * `Err(WorkerError)` if processing failed
    async fn handle(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()>;
}

/// Middleware that wraps message handlers to add cross-cutting concerns.
///
/// Middleware forms a chain where each piece can:
/// - Modify messages before passing them along
/// - Perform actions before and after processing
/// - Short-circuit the chain by returning early
/// - Handle errors and implement retry logic
///
/// # Example
/// ```rust,no_run
/// use foxtive_worker::middleware::{Middleware, MessageHandler};
/// use foxtive_worker::{ReceivedMessage, WorkerResult};
///
/// struct LoggingMiddleware;
///
/// #[async_trait::async_trait]
/// impl Middleware for LoggingMiddleware {
///     fn name(&self) -> &str { "logging" }
///     
///     async fn handle(
///         &self,
///         message: ReceivedMessage<serde_json::Value>,
///         next: Box<dyn MessageHandler>,
///     ) -> WorkerResult<()> {
///         println!("Processing message: {}", message.message.id);
///         let result = next.handle(message).await;
///         println!("Message processed with result: {:?}", result.is_ok());
///         result
///     }
/// }
/// ```
#[async_trait]
pub trait Middleware: Send + Sync {
    /// Get the name of this middleware for debugging and logging.
    fn name(&self) -> &str;

    /// Process a message with access to the next handler in the chain.
    ///
    /// # Arguments
    /// * `message` - The message to process
    /// * `next` - The next handler in the middleware chain
    ///
    /// # Returns
    /// * `Ok(())` if processing succeeded
    /// * `Err(WorkerError)` if processing failed
    async fn handle(
        &self,
        message: ReceivedMessage<serde_json::Value>,
        next: Box<dyn MessageHandler>,
    ) -> WorkerResult<()>;
}

/// A chain of middleware that processes messages in sequence.
///
/// Middleware chains allow you to compose multiple middleware components
/// into a single handler that executes them in order.
pub struct MiddlewareChain {
    middlewares: Vec<Box<dyn Middleware>>,
    final_handler: Box<dyn MessageHandler>,
}

impl MiddlewareChain {
    /// Create a new middleware chain.
    ///
    /// # Arguments
    /// * `middlewares` - Vector of middleware to execute in order
    /// * `final_handler` - The final handler that processes the message
    pub fn new(
        middlewares: Vec<Box<dyn Middleware>>,
        final_handler: Box<dyn MessageHandler>,
    ) -> Self {
        Self {
            middlewares,
            final_handler,
        }
    }

    /// Build the middleware chain into a single handler.
    ///
    /// This creates a nested structure where each middleware wraps the next.
    pub fn build(self) -> Box<dyn MessageHandler> {
        let mut handler: Arc<dyn MessageHandler> = Arc::new(ArcHandlerWrapper(self.final_handler));

        // Wrap handlers in reverse order so first middleware executes first
        for middleware in self.middlewares.into_iter().rev() {
            handler = Arc::new(MiddlewareWrapper {
                middleware,
                inner: handler,
            });
        }

        Box::new(ArcHandler(handler))
    }
}

/// Wrapper that applies a single middleware around an inner handler.
struct MiddlewareWrapper {
    middleware: Box<dyn Middleware>,
    inner: Arc<dyn MessageHandler>,
}

#[async_trait]
impl MessageHandler for MiddlewareWrapper {
    async fn handle(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        let next = self.inner.clone();
        self.middleware
            .handle(message, Box::new(ArcHandler(next)))
            .await
    }
}

/// Helper to wrap Arc<dyn MessageHandler> as Box<dyn MessageHandler>
struct ArcHandler(Arc<dyn MessageHandler>);

#[async_trait]
impl MessageHandler for ArcHandler {
    async fn handle(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        self.0.handle(message).await
    }
}

/// Wrapper to convert Box<dyn MessageHandler> to Arc
struct ArcHandlerWrapper(Box<dyn MessageHandler>);

#[async_trait]
impl MessageHandler for ArcHandlerWrapper {
    async fn handle(&self, message: ReceivedMessage<serde_json::Value>) -> WorkerResult<()> {
        self.0.handle(message).await
    }
}
