# Foxtive-Macros Changelog
Foxtive macros lib changelog file

### 0.5.0 (2026-08-19)
* feat(macros): add #[derive(Service)] that auto-generates ServiceInit from struct field declarations
* feat(macros): support #[service(all)] mode — all fields treated as deps, opt out with #[foxtive(default)]
* feat(macros): support opt-in mode — only #[dependency] fields resolved from container
* feat(macros): support #[foxtive(init = "expr")] for declarative field initialization from app config
* feat(macros): support #[service(mutable)] for automatic Mutable<T> wrapping
* feat(macros): support #[service(skip_hooks)] for custom ServiceHooks implementations
* feat(macros): auto-detect infrastructure types (DBPool, Redis, RabbitMQ, Cache, Jwt, Password)
* feat(macros): auto-detect Lazy<T> fields for deferred wiring
* feat(macros): auto-detect Option<T> fields for optional dependencies
* feat(macros): auto-detect Arc<dyn Trait> fields for trait binding resolution

### 0.4.3 (2026-04-06)
* feat(enum): auto derive Debug to enums

### 0.4.2 (2025-01-11)
* feat(diesel): 'generate_diesel_enum_with_optional_features' should be available without database feature enabled

### 0.4.1 (2024-10-13)
* feat(enums): now implements Copy by default

### 0.4.0 (2024-06-05)
* bump(edition): to 2024
