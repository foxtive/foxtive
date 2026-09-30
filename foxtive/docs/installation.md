## Foxtive Installation

```toml
foxtive = { version = "1.4" }
```

## Loading Environment Files

Use `App::load_env_files` before building the app. Loading is fail-fast: every
listed file must exist, and the error names the offending path. `dotenvy` never
overwrites already-set variables, so the first file that sets a key wins - pass
the most-specific file first:

```rust
use foxtive::App;

# fn run() -> foxtive::results::AppResult<()> {
// Returns a prefix-bound ScopedEnv for immediate reads.
let env = App::load_env_files("MYAPP", ["apps/my-service/.env", ".env"])?;
let db_host = env.var("DB_HOST")?;          // reads MYAPP_DB_HOST
let port: u16 = env.parse_or("PORT", 8080); // MYAPP_PORT with fallback
# Ok(())
# }
```

Alternatively, record files on the builder - they load at the start of `build()`:

```rust
use foxtive::App;

# async fn run() -> foxtive::results::AppResult<()> {
let app = App::builder("my-service", "MYSVC")
    .env_prefix("MYAPP")
    .env_files(["apps/my-service/.env", ".env"])
    .require_env(["DB_DSN"])
    .build()
    .await?;

let port: u16 = app.env_vars().parse("SERVER_PORT")?; // MYAPP_SERVER_PORT
// ScopedEnv is registered in the DI container too:
let env = app.require::<foxtive::ScopedEnv>()?;
# Ok(())
# }
```

Note: `ScopedEnv` (the env-var reader) is distinct from `Environment` (the
`Local`/`Dev`/`Prod` enum returned by `app.env()`).