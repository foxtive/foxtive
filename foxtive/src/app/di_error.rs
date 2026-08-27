//! Structured DI error types with rustc-style formatting.
//!
//! Each [`DiError`] variant carries structured data (not pre-formatted strings).
//! The `Display` impl produces human-readable rustc-style output.
//! Pattern matching on variants gives programmatic access.
//!
//! # Error Codes
//!
//! - `DI0001` - Circular runtime dependency (undeclared deps form a cycle)
//! - `DI0002` - Declared circular dependency (visible in `dependencies()`)
//! - `DI0003` - Service construction failed (terminal init error)

use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;

use crate::app::deps::ServiceResolutionError;
use crate::enums::AppMessage;
use crate::lifecycle::ServiceFactory;

/// A blocked service entry: `(service_name, what_it_needs)`.
pub(crate) type BlockedService = (String, String);
/// A terminal error entry: `(service_name, error_message)`.
pub(crate) type TerminalError = (String, String);

/// Returns `true` when stderr is a TTY (colors are safe to emit).
fn use_color() -> bool {
    std::io::stderr().is_terminal()
}

#[inline]
fn paint(ansi: &str, text: &str) -> String {
    if use_color() {
        format!("{ansi}{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// rustc-style color palette.
mod style {
    /// Red + bold - used for `error`.
    pub const ERROR: &str = "\x1b[1;31m";
    /// Yellow - used for error codes like `[DI0001]`.
    pub const CODE: &str = "\x1b[1;33m";
    /// Cyan + bold - used for `-->`, `|`, `= note:`, `= help:`.
    pub const LABEL: &str = "\x1b[1;36m";
    /// Green - used for suggested fix text.
    pub const HELP: &str = "\x1b[1;32m";
    /// Bold - used for emphasis (service names in chains, cycle numbers).
    pub const BOLD: &str = "\x1b[1m";
}

/// Structured DI error with rustc-style formatting.
///
/// Each variant carries structured data (not pre-formatted strings).
/// The `Display` impl produces the human-readable rustc-style output.
/// Pattern matching on variants gives programmatic access.
///
/// # Error Codes
///
/// - `DI0001` - Circular runtime dependency (undeclared deps form a cycle)
/// - `DI0002` - Declared circular dependency (visible in `dependencies()`)
/// - `DI0003` - Service construction failed (terminal init error)
#[derive(thiserror::Error)]
pub enum DiError {
    /// DI0001: Undeclared runtime dependency cycle detected in Phase 2.
    ///
    /// Phase 1a (DFS on declared deps) succeeded, but services formed
    /// a cycle through undeclared runtime dependencies (discovered in `init()`).
    #[error("{}", format_di0001(self))]
    CircularRuntimeDependency {
        /// Detected cycles, each a list of short type names in chain order.
        /// e.g., `[["AuthService", "LoginService", "UserDeviceService"]]`
        cycles: Vec<Vec<String>>,

        /// Non-cycle services blocked by the deadlock, as `(service, what_it_needs)`.
        /// Computed from BOTH the `DependencyMissing` error AND the service's
        /// declared `dependencies()` list checked against `constructed_set`,
        /// so the full picture is shown - not just the first missing dep.
        blocked_services: Vec<BlockedService>,

        /// Dependencies that are not registered as services at all: `(service, unregistered_dep)`.
        /// These are root-cause failures - registering these services would unblock
        /// the entire dependency chain.
        unregistered_deps: Vec<(String, String)>,
    },

    /// DI0002: Declared dependencies form a cycle (detected in Phase 1a DFS).
    ///
    /// This is a compile-time-equivalent error - the cycle is visible in
    /// `dependencies()` declarations and can be fixed without runtime inspection.
    #[error("{}", format_di0002(self))]
    DeclaredCircularDependency {
        /// Cycle chain as short type names, e.g., `["ServiceA", "ServiceB", "ServiceA"]`.
        chain: Vec<String>,
    },

    /// DI0003: A service factory returned a terminal (non-retryable) error.
    #[error("{}", format_di0003(self))]
    ServiceConstructionFailed {
        /// Short type name of the service that failed.
        service: String,
        /// The original error from `init()` / `after_init()`.
        #[source]
        source: Box<AppMessage>,
    },
}

/// Custom `Debug` that delegates to `Display` for pretty rustc-style output.
///
/// This ensures errors look good whether printed with `{}` or `{:?}` -
/// important when errors propagate to `main()` (which uses `Debug`).
impl std::fmt::Debug for DiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<DiError> for AppMessage {
    fn from(e: DiError) -> Self {
        AppMessage::DiError(e)
    }
}

fn format_di0001(e: &DiError) -> String {
    let DiError::CircularRuntimeDependency {
        cycles,
        blocked_services,
        unregistered_deps,
    } = e
    else {
        unreachable!()
    };

    let mut out = String::new();

    let err_label = paint(style::ERROR, "error");
    let code = paint(style::CODE, "[DI0001]");
    let bar = paint(style::LABEL, "|");
    let arrow = paint(style::LABEL, "-->");
    let note_label = paint(style::LABEL, "= note:");
    let help_label = paint(style::LABEL, "= help:");

    if cycles.is_empty() {
        out.push_str(&format!(
            "{err_label}{code}: service construction deadlock detected\n"
        ));
    } else {
        out.push_str(&format!(
            "{err_label}{code}: circular runtime dependency detected\n"
        ));
    }

    // --> pointer: first service in first cycle, or first blocked service
    let pointer = cycles
        .first()
        .and_then(|c| c.first())
        .map(|s| s.as_str())
        .or_else(|| blocked_services.first().map(|(s, _)| s.as_str()))
        .unwrap_or("unknown");

    out.push_str(&format!("  {arrow} {pointer}\n"));
    out.push_str(&format!("   {bar}\n"));

    if cycles.is_empty() {
        // Non-cycle deadlock
        out.push_str(&format!(
            "   {bar}  {note_label} Services have dependencies that could not be resolved.\n"
        ));

        // Show unregistered (root-cause) deps first
        if !unregistered_deps.is_empty() {
            out.push_str(&format!("   {bar}\n"));
            out.push_str(&format!(
                "   {bar}  The following dependencies are not registered as services:\n"
            ));
            // Group by unregistered dep name
            let mut unique_unregistered: Vec<&str> = unregistered_deps
                .iter()
                .map(|(_, dep)| dep.as_str())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            unique_unregistered.sort();
            for dep_name in &unique_unregistered {
                let dep_bold = paint(style::BOLD, dep_name);
                // Show which services need this unregistered dep
                let needed_by: Vec<&str> = unregistered_deps
                    .iter()
                    .filter(|(_, d)| d.as_str() == *dep_name)
                    .map(|(svc, _)| svc.as_str())
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect();
                let needed_by_str = needed_by.join(", ");
                out.push_str(&format!(
                    "   {bar}    • {dep_bold} - required by {needed_by_str}\n"
                ));
            }
        }

        // Show blocked services (registered but can't be constructed)
        if !blocked_services.is_empty() {
            out.push_str(&format!("   {bar}\n"));
            out.push_str(&format!(
                "   {bar}  Blocked services (waiting for dependencies):\n"
            ));
            // Group: show each service once with all its deps
            let mut service_deps: Vec<(&str, Vec<&str>)> = Vec::new();
            for (svc, dep) in blocked_services {
                if let Some(entry) = service_deps.iter_mut().find(|(s, _)| *s == svc.as_str()) {
                    if !entry.1.contains(&dep.as_str()) {
                        entry.1.push(dep.as_str());
                    }
                } else {
                    service_deps.push((svc.as_str(), vec![dep.as_str()]));
                }
            }
            service_deps.sort_by_key(|(s, _)| *s);
            for (service, deps) in &service_deps {
                let service_bold = paint(style::BOLD, service);
                let deps_str = deps.join(", ");
                let deps_bold = paint(style::BOLD, &deps_str);
                out.push_str(&format!(
                    "   {bar}    • {service_bold} - requires {deps_bold}\n"
                ));
            }
        }

        out.push_str(&format!("   {bar}\n"));
        let lazy_help = paint(style::HELP, "Lazy<T>");
        if !unregistered_deps.is_empty() {
            out.push_str(&format!(
                "   {bar}  {help_label} Register the missing services, or provide them via\n"
            ));
            out.push_str(&format!(
                "   {bar}         app.register() before calling build().\n"
            ));
            out.push_str(&format!("   {bar}         If services depend on each other at runtime, wrap one side in {lazy_help}.\n"));
        } else {
            out.push_str(&format!("   {bar}  {help_label} If services depend on each other at runtime, wrap one side in {lazy_help}.\n"));
            out.push_str(&format!("   {bar}         Declare dependencies via #[dependency] or dependencies() to fix ordering.\n"));
        }
    } else {
        // Cycle deadlock
        out.push_str(&format!("   {bar}  {note_label} Services form a dependency cycle that cannot be resolved at runtime.\n"));
        out.push_str(&format!(
            "   {bar}          Phase 1 construction (topological sort) succeeded, but runtime\n"
        ));
        out.push_str(&format!("   {bar}          dependencies discovered during init() create an unresolvable cycle.\n"));
        out.push_str(&format!("   {bar}\n"));

        for (i, cycle) in cycles.iter().enumerate() {
            out.push_str(&format!("   {bar}  Cycle {}:\n", i + 1));
            let chain = cycle.join(" → ");
            let closing = &cycle[0];
            let chain_bold = paint(style::BOLD, &format!("{chain} → {closing}"));
            out.push_str(&format!("   {bar}    {chain_bold}\n"));
            out.push_str(&format!("   {bar}\n"));
        }

        let lazy_help = paint(style::HELP, "Lazy<T>");
        out.push_str(&format!(
            "   {bar}  {help_label} Wrap one of these in {lazy_help} to break the cycle:\n"
        ));
        if let Some(first_cycle) = cycles.first() {
            for name in first_cycle {
                let name_bold = paint(style::BOLD, name);
                out.push_str(&format!("   {bar}    • {name_bold}\n"));
            }
        }

        if !blocked_services.is_empty() {
            out.push_str(&format!("   {bar}\n"));
            out.push_str(&format!(
                "   {bar}  {note_label} {} services blocked by this cycle:\n",
                blocked_services.len()
            ));
            for (service, needs) in blocked_services {
                let service_bold = paint(style::BOLD, service);
                let needs_bold = paint(style::BOLD, needs);
                out.push_str(&format!(
                    "   {bar}    • {service_bold} - needs {needs_bold}\n"
                ));
            }
        }
    }

    out
}

fn format_di0002(e: &DiError) -> String {
    let DiError::DeclaredCircularDependency { chain } = e else {
        unreachable!()
    };

    let mut out = String::new();

    let err_label = paint(style::ERROR, "error");
    let code = paint(style::CODE, "[DI0002]");
    let bar = paint(style::LABEL, "|");
    let arrow = paint(style::LABEL, "-->");
    let note_label = paint(style::LABEL, "= note:");
    let help_label = paint(style::LABEL, "= help:");

    out.push_str(&format!(
        "{err_label}{code}: declared circular dependency in service graph\n"
    ));

    let pointer = chain.first().map(|s| s.as_str()).unwrap_or("unknown");
    out.push_str(&format!("  {arrow} {pointer}\n"));
    out.push_str(&format!(
        "   {bar}  {note_label} The `{arrow}` pointer shows the first service in the cycle chain\n"
    ));
    out.push_str(&format!(
        "   {bar}          (the one where DFS detected the back-edge).\n"
    ));
    out.push_str(&format!("   {bar}\n"));
    out.push_str(&format!(
        "   {bar}  {note_label} Declared dependencies form a cycle that cannot be resolved:\n"
    ));
    out.push_str(&format!("   {bar}\n"));
    out.push_str(&format!("   {bar}  Dependency chain:\n"));
    let chain_str = chain.join(" \u{2192} ");
    let chain_bold = paint(style::BOLD, &chain_str);
    out.push_str(&format!("   {bar}    {chain_bold}\n"));

    out.push_str(&format!("   {bar}\n"));
    out.push_str(&format!(
        "   {bar}  {help_label} This is detected during topological sort (Phase 1a) before any\n"
    ));
    out.push_str(&format!(
        "   {bar}          services are constructed. To fix:\n"
    ));
    out.push_str(&format!("   {bar}\n"));
    out.push_str(&format!(
        "   {bar}  Option 1: Remove the circular dependency\n"
    ));
    out.push_str(&format!(
        "   {bar}    Review if both services truly need each other. Consider extracting\n"
    ));
    out.push_str(&format!("   {bar}    shared logic into a third service.\n"));
    out.push_str(&format!("   {bar}\n"));
    let lazy_type_header = format!("Lazy<{}>", paint(style::BOLD, &chain[1]));
    out.push_str(&format!(
        "   {bar}  Option 2: Use {lazy_type_help} for one direction\n",
        lazy_type_help = paint(style::HELP, &lazy_type_header)
    ));

    if chain.len() >= 2 {
        let svc0 = paint(style::BOLD, &chain[0]);
        out.push_str(&format!("   {bar}    In {svc0}, change:\n"));
        let snake = to_snake_case(&chain[1]);
        let svc1 = paint(style::BOLD, &chain[1]);
        out.push_str(&format!("   {bar}      {snake}: Arc<{svc1}>\n"));
        out.push_str(&format!("   {bar}    to:\n"));
        let lazy_type_suggest = format!("Lazy<{}>", paint(style::BOLD, &chain[1]));
        let lazy_type_styled = paint(style::HELP, &lazy_type_suggest);
        out.push_str(&format!("   {bar}      {snake}: {lazy_type_styled}\n"));
    }

    out
}

fn format_di0003(e: &DiError) -> String {
    let DiError::ServiceConstructionFailed { service, source } = e else {
        unreachable!()
    };

    let mut out = String::new();

    let err_label = paint(style::ERROR, "error");
    let code = paint(style::CODE, "[DI0003]");
    let bar = paint(style::LABEL, "|");
    let arrow = paint(style::LABEL, "-->");
    let note_label = paint(style::LABEL, "= note:");
    let help_label = paint(style::LABEL, "= help:");
    let service_bold = paint(style::BOLD, service);

    out.push_str(&format!("{err_label}{code}: service construction failed\n"));
    out.push_str(&format!("  {arrow} {service_bold}\n"));
    out.push_str(&format!("   {bar}\n"));
    out.push_str(&format!(
        "   {bar}  {note_label} Service initialization returned an error:\n"
    ));
    out.push_str(&format!("   {bar}          {source}\n"));
    out.push_str(&format!("   {bar}\n"));
    out.push_str(&format!(
        "   {bar}  {help_label} This is a terminal error - the service cannot be constructed.\n"
    ));
    out.push_str(&format!("   {bar}         Check:\n"));
    out.push_str(&format!(
        "   {bar}    • The service's dependencies are correctly configured\n"
    ));
    out.push_str(&format!(
        "   {bar}    • External resources (database, cache, etc.) are accessible\n"
    ));
    out.push_str(&format!(
        "   {bar}    • Environment variables are set correctly\n"
    ));
    out.push_str(&format!("   {bar}\n"));
    out.push_str(&format!("   {bar}  Original error:\n"));
    out.push_str(&format!("   {bar}    {source}\n"));

    out
}

/// Convert a type name to a rough snake_case field name for suggestions.
/// e.g., "AuthService" → "auth_service"
fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    for (i, ch) in name.chars().enumerate() {
        if ch.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(ch.to_ascii_lowercase());
    }
    result
}

/// Extract the short type name from a fully-qualified path.
///
/// `"ocean::services::auth_service::AuthService"` → `"AuthService"`
///
/// If there are no `::` separators, returns the input unchanged.
pub(crate) fn short_type_name(fq_name: &str) -> &str {
    fq_name.rsplit("::").next().unwrap_or(fq_name)
}

/// Extract the type name from a `missing_type` error string.
///
/// The `missing_type` field is formatted by `app.require()` as one of:
/// - `"Service of type X not registered"`
/// - `"Service of type X not found"`
///
/// Returns `"X"` (the fully-qualified type name).
pub(crate) fn parse_missing_type_name(missing_type: &str) -> String {
    missing_type
        .strip_prefix("Service of type ")
        .and_then(|s| s.find(" not ").map(|pos| s[..pos].to_string()))
        .unwrap_or_else(|| missing_type.to_string())
}

/// Compute the full list of blocked services for deadlock error reporting.
///
/// Walks `last_errors` entries NOT in `cycle_members`. For each blocked service,
/// checks ALL declared `dependencies()` against `constructed_set` - any dep whose
/// factory index is NOT in `constructed_set` is reported as missing. Also extracts
/// the runtime (undeclared) missing dep from `DependencyMissing.missing_type`.
///
/// Returns `(blocked_services, terminal_errors)` - two separate lists so the
/// caller can format them differently.
pub(crate) fn compute_blocked_services(
    last_errors: &HashMap<usize, ServiceResolutionError>,
    factories: &[Box<dyn ServiceFactory>],
    cycle_members: &HashSet<usize>,
    constructed_set: &HashSet<usize>,
) -> (Vec<BlockedService>, Vec<TerminalError>) {
    let mut blocked: Vec<BlockedService> = Vec::new();
    let mut terminal_errors: Vec<TerminalError> = Vec::new();

    // Build type_name → factory_idx map for resolving declared dep names
    let type_to_idx: HashMap<&str, usize> = factories
        .iter()
        .enumerate()
        .map(|(i, f)| (f.type_name(), i))
        .collect();

    for (&idx, err) in last_errors {
        if cycle_members.contains(&idx) {
            continue;
        }

        let service_name = short_type_name(factories[idx].type_name()).to_string();

        match err {
            ServiceResolutionError::DependencyMissing { missing_type, .. } => {
                // 1. Runtime (undeclared) missing dep from the error
                let missing_fq = parse_missing_type_name(missing_type);
                let short_missing = short_type_name(&missing_fq).to_string();
                blocked.push((service_name.clone(), short_missing));

                // 2. Check ALL declared deps against constructed_set
                for dep_name in factories[idx].dependencies() {
                    if let Some(&dep_idx) = type_to_idx.get(dep_name)
                        && !constructed_set.contains(&dep_idx)
                    {
                        let dep_short = short_type_name(factories[dep_idx].type_name()).to_string();
                        blocked.push((service_name.clone(), dep_short));
                    }
                }
            }
            ServiceResolutionError::Terminal(e) => {
                terminal_errors.push((service_name, e.to_string()));
            }
        }
    }

    // Deduplicate
    blocked.sort();
    blocked.dedup();

    (blocked, terminal_errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::StatusCode;

    /// Strip ANSI escape sequences so assertions work regardless of TTY.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip ESC [ ... final byte (0x40-0x7E)
                if chars.next() == Some('[') {
                    for p in chars.by_ref() {
                        if (0x40..=0x7E).contains(&(p as u32)) {
                            break;
                        }
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    #[test]
    fn test_short_type_name() {
        assert_eq!(
            short_type_name("ocean::services::auth_service::AuthService"),
            "AuthService"
        );
        assert_eq!(short_type_name("SimpleService"), "SimpleService");
        assert_eq!(short_type_name("a::B"), "B");
    }

    #[test]
    fn test_parse_missing_type_name() {
        assert_eq!(
            parse_missing_type_name("Service of type ocean::services::AuthService not registered"),
            "ocean::services::AuthService",
        );
        assert_eq!(
            parse_missing_type_name("Service of type Foo not found"),
            "Foo",
        );
        // Fallback for unexpected format
        assert_eq!(
            parse_missing_type_name("something unexpected"),
            "something unexpected",
        );
    }

    #[test]
    fn test_di0001_cycle_display() {
        let err = DiError::CircularRuntimeDependency {
            cycles: vec![vec![
                "AuthService".into(),
                "LoginService".into(),
                "UserDeviceService".into(),
            ]],
            blocked_services: vec![
                ("UserService".into(), "AuthService".into()),
                ("TeamService".into(), "UserService".into()),
            ],
            unregistered_deps: vec![],
        };
        let text = strip_ansi(&err.to_string());
        assert!(
            text.contains("error[DI0001]"),
            "should have error code: {text}"
        );
        assert!(
            text.contains("circular runtime dependency"),
            "should mention cycle: {text}"
        );
        assert!(text.contains("Cycle 1:"), "should label cycle: {text}");
        assert!(
            text.contains("AuthService → LoginService → UserDeviceService"),
            "should show cycle chain: {text}"
        );
        assert!(text.contains("Lazy<T>"), "should suggest fix: {text}");
        assert!(
            text.contains("2 services blocked"),
            "should show impact: {text}"
        );
        assert!(!text.contains("ocean::"), "should use short names: {text}");
    }

    #[test]
    fn test_di0001_non_cycle_display() {
        let err = DiError::CircularRuntimeDependency {
            cycles: vec![],
            blocked_services: vec![
                ("AuthService".into(), "UserService".into()),
                ("UserService".into(), "SomeOtherService".into()),
            ],
            unregistered_deps: vec![("UserService".into(), "SomeOtherService".into())],
        };
        let text = strip_ansi(&err.to_string());
        assert!(
            text.contains("error[DI0001]"),
            "should have error code: {text}"
        );
        assert!(
            text.contains("service construction deadlock"),
            "should mention deadlock: {text}"
        );
        assert!(
            text.contains("not registered"),
            "should show unregistered: {text}"
        );
        assert!(text.contains("Lazy<T>"), "should suggest fix: {text}");
    }

    #[test]
    fn test_di0002_display() {
        let err = DiError::DeclaredCircularDependency {
            chain: vec!["ServiceA".into(), "ServiceB".into(), "ServiceA".into()],
        };
        let text = strip_ansi(&err.to_string());
        assert!(
            text.contains("error[DI0002]"),
            "should have error code: {text}"
        );
        assert!(
            text.contains("declared circular dependency"),
            "should describe error: {text}"
        );
        assert!(
            text.contains("ServiceA \u{2192} ServiceB \u{2192} ServiceA"),
            "should show dep chain: {text}"
        );
        assert!(
            text.contains("Lazy<ServiceB>"),
            "should suggest concrete Lazy type: {text}"
        );
    }

    #[test]
    fn test_di0003_display() {
        let err = DiError::ServiceConstructionFailed {
            service: "DatabaseService".into(),
            source: Box::new(AppMessage::Infrastructure {
                message: "connection refused".into(),
                source: None,
            }),
        };
        let text = strip_ansi(&err.to_string());
        assert!(
            text.contains("error[DI0003]"),
            "should have error code: {text}"
        );
        assert!(
            text.contains("service construction failed"),
            "should describe error: {text}"
        );
        assert!(
            text.contains("DatabaseService"),
            "should show service name: {text}"
        );
        assert!(
            text.contains("connection refused"),
            "should show original error: {text}"
        );
    }

    #[test]
    fn test_di_error_to_app_message_conversion() {
        let err = DiError::DeclaredCircularDependency {
            chain: vec!["A".into(), "B".into(), "A".into()],
        };
        let msg: AppMessage = err.into();
        assert_eq!(msg.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(msg.kind_name(), "di_error");
    }
}
