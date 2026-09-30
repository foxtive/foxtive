//! Panic hook installation with environment-aware logging.
//!
//! Installs a [`std::panic`](mod@std::panic) hook that logs the panic payload and location via
//! `tracing::error!`. In environments where [`Environment::allows_debug()`] is
//! true (anything but production), a [`std::backtrace::Backtrace`] is captured
//! and included. In production the backtrace is omitted and a structured
//! single-line log is emitted instead.
//!
//! Enable via [`AppBuilder::panic_hook(true)`](crate::AppBuilder::panic_hook),
//! which calls [`install`] during `build()`, or call [`install`] directly.

use crate::Environment;

/// Install the foxtive panic hook for the given environment.
///
/// Replaces any previously installed hook. Called automatically during
/// `build()` when [`AppBuilder::panic_hook(true)`](crate::AppBuilder::panic_hook)
/// was set.
pub fn install(env: Environment) {
    std::panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());

        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown location>".to_string());

        if env.allows_debug() {
            // Capture *inside* the hook: a backtrace taken at install time is useless.
            let backtrace = std::backtrace::Backtrace::force_capture();
            tracing::error!(
                panic.payload = payload,
                panic.location = location,
                environment = env.as_str(),
                "Panic occurred\nbacktrace:\n{backtrace}"
            );
        } else {
            tracing::error!(
                panic.payload = payload,
                panic.location = location,
                environment = env.as_str(),
                "Panic occurred"
            );
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_hook_survives_panic() {
        // Any prior hook is replaced; after the test the default hook remains.
        // This is process-global - acceptable here since no other test in this
        // module asserts on hook identity.
        install(Environment::Local);

        let result = std::panic::catch_unwind(|| {
            panic!("test panic payload");
        });

        assert!(result.is_err(), "catch_unwind should capture the panic");
        // The hook ran without aborting; the panic payload round-trips.
        let err = result.unwrap_err();
        let payload = err.downcast_ref::<&str>().map(|s| (*s).to_string());
        assert_eq!(payload.as_deref(), Some("test panic payload"));
    }

    #[test]
    fn test_install_production_variant() {
        install(Environment::Production);

        let result = std::panic::catch_unwind(|| {
            // panic_any produces a String payload (not &str) to exercise that arm.
            std::panic::panic_any(String::from("owned string payload"));
        });

        assert!(result.is_err());
    }
}
