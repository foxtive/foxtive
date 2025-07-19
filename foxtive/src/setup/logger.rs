use crate::prelude::AppResult;
use std::sync::Arc;
use tracing::Level;
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use crate::setup::logger_layers::EventCallbackLayer;

pub type LoggerEventHandler = Arc<dyn Fn(&tracing::Event<'_>) + Send + Sync + 'static>;

#[derive(Clone)]
pub struct TracingConfig {
    pub level: Level,
    pub format: OutputFormat,
    pub target: OutputTarget,
    pub include_file: bool,
    pub include_line_number: bool,
    pub include_target: bool,
    pub include_thread_ids: bool,
    pub include_thread_names: bool,
    pub enable_ansi: bool,
    pub on_logger_event: Option<LoggerEventHandler>,
}

impl std::fmt::Debug for TracingConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TracingConfig")
            .field("level", &self.level)
            .field("format", &self.format)
            .field("target", &self.target)
            .field("include_file", &self.include_file)
            .field("include_line_number", &self.include_line_number)
            .field("include_target", &self.include_target)
            .field("include_thread_ids", &self.include_thread_ids)
            .field("include_thread_names", &self.include_thread_names)
            .field("enable_ansi", &self.enable_ansi)
            .field(
                "on_event",
                &self.on_logger_event.as_ref().map(|_| "<callback>"),
            )
            .finish()
    }
}

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Pretty,
    Json,
    Compact,
    Full,
}

#[derive(Debug, Clone)]
pub enum OutputTarget {
    Stdout,
    Stderr,
    File(String),
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            format: OutputFormat::Pretty,
            target: OutputTarget::Stdout,
            include_file: true,
            include_line_number: true,
            include_target: true,
            include_thread_ids: false,
            include_thread_names: true,
            enable_ansi: true,
            on_logger_event: None,
        }
    }
}

pub fn init_tracing(config: TracingConfig) -> AppResult<()> {
    macro_rules! init_subscriber {
        ($fmt_layer:expr) => {
            let env_filter = EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new(config.level.to_string()))?;

            if let Some(on_logger_event) = config.on_logger_event {
                tracing_subscriber::registry()
                    .with(EventCallbackLayer::new(on_logger_event))
                    .with(env_filter)
                    .with($fmt_layer)
                    .init();
            } else {
                tracing_subscriber::registry()
                    .with(env_filter)
                    .with($fmt_layer)
                    .init();
            }
        };
    }

    match (config.format, config.target) {
        (OutputFormat::Json, OutputTarget::Stdout) => {
            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(config.enable_ansi)
            );
        }
        (OutputFormat::Json, OutputTarget::Stderr) => {
            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(config.enable_ansi)
                    .with_writer(std::io::stderr)
            );
        }
        (OutputFormat::Json, OutputTarget::File(path)) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;

            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_span_list(true)
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(false)
                    .with_writer(file)
            );
        }
        (OutputFormat::Pretty, OutputTarget::Stdout) => {
            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(config.enable_ansi)
            );
        }
        (OutputFormat::Pretty, OutputTarget::Stderr) => {
            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(config.enable_ansi)
                    .with_writer(std::io::stderr)
            );
        }
        (OutputFormat::Pretty, OutputTarget::File(path)) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;

            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .pretty()
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(false)
                    .with_writer(file)
            );
        }
        (OutputFormat::Compact, OutputTarget::Stdout) => {
            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(config.enable_ansi)
            );
        }
        (OutputFormat::Compact, OutputTarget::Stderr) => {
            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(config.enable_ansi)
                    .with_writer(std::io::stderr)
            );
        }
        (OutputFormat::Compact, OutputTarget::File(path)) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;

            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(false)
                    .with_writer(file)
            );
        }
        (OutputFormat::Full, OutputTarget::Stdout) => {
            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(config.enable_ansi)
            );
        }
        (OutputFormat::Full, OutputTarget::Stderr) => {
            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(config.enable_ansi)
                    .with_writer(std::io::stderr)
            );
        }
        (OutputFormat::Full, OutputTarget::File(path)) => {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;

            init_subscriber!(
                tracing_subscriber::fmt::layer()
                    .with_file(config.include_file)
                    .with_line_number(config.include_line_number)
                    .with_target(config.include_target)
                    .with_thread_ids(config.include_thread_ids)
                    .with_thread_names(config.include_thread_names)
                    .with_ansi(false)
                    .with_writer(file)
            );
        }
    }

    Ok(())
}

impl TracingConfig {
    pub fn with_logger_event_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(&tracing::Event<'_>) + Send + Sync + 'static,
    {
        self.on_logger_event = Some(Arc::new(callback));
        self
    }

    pub fn with_logger_event_callback_arc(
        mut self,
        callback: Arc<dyn Fn(&tracing::Event<'_>) + Send + Sync + 'static>,
    ) -> Self {
        self.on_logger_event = Some(callback);
        self
    }

    pub fn with_output_format(mut self, format: OutputFormat) -> Self {
        self.format = format;
        self
    }

    pub fn with_output_target(mut self, target: OutputTarget) -> Self {
        self.target = target;
        self
    }

    pub fn with_enable_ansi(mut self, state: bool) -> Self {
        self.enable_ansi = state;
        self
    }

    pub fn with_include_file(mut self, state: bool) -> Self {
        self.include_file = state;
        self
    }

    pub fn with_include_line_number(mut self, state: bool) -> Self {
        self.include_line_number = state;
        self
    }

    pub fn with_include_target(mut self, state: bool) -> Self {
        self.include_target = state;
        self
    }

    pub fn with_include_thread_ids(mut self, state: bool) -> Self {
        self.include_thread_ids = state;
        self
    }

    pub fn with_include_thread_names(mut self, state: bool) -> Self {
        self.include_thread_names = state;
        self
    }

    /// Hide all location information (file, line number, and target)
    pub fn hide_location_info(mut self) -> Self {
        self.include_file = false;
        self.include_line_number = false;
        self.include_target = false;
        self
    }

    /// Show all location information (file, line number, and target)
    pub fn show_location_info(mut self) -> Self {
        self.include_file = true;
        self.include_line_number = true;
        self.include_target = true;
        self
    }

    /// Create a minimal configuration with only essential information
    pub fn minimal() -> Self {
        Self {
            level: Level::INFO,
            format: OutputFormat::Compact,
            target: OutputTarget::Stdout,
            include_file: false,
            include_line_number: false,
            include_target: false,
            include_thread_ids: false,
            include_thread_names: false,
            enable_ansi: true,
            on_logger_event: None,
        }
    }

    /// Create a verbose configuration with all information
    pub fn verbose() -> Self {
        Self {
            level: Level::DEBUG,
            format: OutputFormat::Full,
            target: OutputTarget::Stdout,
            include_file: true,
            include_line_number: true,
            include_target: true,
            include_thread_ids: true,
            include_thread_names: true,
            enable_ansi: true,
            on_logger_event: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tracing::{error, info, warn};

    #[test]
    fn test_event_callback() {
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();

        let config = TracingConfig::default().with_logger_event_callback(move |event| {
            let level = event.metadata().level();
            let target = event.metadata().target();
            println!("Event callback triggered: level={level}, target={target}");

            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        init_tracing(config).expect("Failed to initialize tracing");

        info!("This is an info message");
        warn!("This is a warning message");
        error!("This is an error message");

        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn test_minimal_config() {
        let config = TracingConfig::minimal();
        assert!(!config.include_file);
        assert!(!config.include_line_number);
        assert!(!config.include_target);
        assert!(!config.include_thread_ids);
        assert!(!config.include_thread_names);
    }

    #[test]
    fn test_verbose_config() {
        let config = TracingConfig::verbose();
        assert!(config.include_file);
        assert!(config.include_line_number);
        assert!(config.include_target);
        assert!(config.include_thread_ids);
        assert!(config.include_thread_names);
        assert_eq!(config.level, Level::DEBUG);
    }

    #[test]
    fn test_hide_location_info() {
        let config = TracingConfig::default().hide_location_info();
        assert!(!config.include_file);
        assert!(!config.include_line_number);
        assert!(!config.include_target);
    }

    #[test]
    fn test_show_location_info() {
        let config = TracingConfig::minimal().show_location_info();
        assert!(config.include_file);
        assert!(config.include_line_number);
        assert!(config.include_target);
    }
}
