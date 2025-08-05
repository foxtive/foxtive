use std::sync::Arc;

pub(crate) struct EventCallbackLayer {
    callback: Arc<dyn Fn(&tracing::Event<'_>) + Send + Sync + 'static>,
}

impl EventCallbackLayer {
    pub fn new(callback: Arc<dyn Fn(&tracing::Event<'_>) + Send + Sync + 'static>) -> Self {
        Self { callback }
    }
}

impl<S> tracing_subscriber::Layer<S> for EventCallbackLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        (self.callback)(event);
    }
}
