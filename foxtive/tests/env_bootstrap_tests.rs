//! Integration tests for the env bootstrap surface:
//! `App::load_env_files`, `ScopedEnv` (incl. DI registration), builder
//! `.env_file()`/`.env_files()`, `.require_env()`, `.config()`/`.config_file()`,
//! and `env_vars()` accessors on `AppBuilder` / `App` / `AppInit`.
//!
//! Env is process-global and `set_var`/`remove_var` are `unsafe` in edition 2024,
//! so every env-mutating test serializes on `ENV_LOCK` and uses a unique prefix.
//!
//! The lock is deliberately held across `.await` in the async tests: the whole
//! point is to serialize env mutation for the entire build, and an async mutex
//! would not serialize against the non-async unit tests. Hence the allow below.
#![allow(clippy::await_holding_lock)]

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use foxtive::{App, ScopedEnv};
use serde::Deserialize;

/// Serializes all env-mutating tests in this binary.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn guard() -> MutexGuard<'static, ()> {
    ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn set(key: &str, value: &str) {
    unsafe { std::env::set_var(key, value) };
}

fn remove(key: &str) {
    unsafe { std::env::remove_var(key) };
}

// ---------------------------------------------------------------------------
// App::load_env_files
// ---------------------------------------------------------------------------

#[test]
fn load_env_files_returns_prefix_bound_scoped_env() {
    let _g = guard();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".env");
    std::fs::write(&file, "FX_IT_LOAD_DB_HOST=db.internal\n").unwrap();

    let env = App::load_env_files("FX_IT_LOAD", [&file]).unwrap();
    assert_eq!(env.prefix(), "FX_IT_LOAD");
    assert_eq!(env.key("DB_HOST"), "FX_IT_LOAD_DB_HOST");
    assert_eq!(env.var("DB_HOST").unwrap(), "db.internal");

    remove("FX_IT_LOAD_DB_HOST");
}

#[test]
fn load_env_files_first_file_wins() {
    let _g = guard();
    let dir = tempfile::tempdir().unwrap();
    let specific = dir.path().join("specific.env");
    let root = dir.path().join("root.env");
    std::fs::write(&specific, "FX_IT_LAYER_KEY=from-specific\n").unwrap();
    std::fs::write(
        &root,
        "FX_IT_LAYER_KEY=from-root\nFX_IT_LAYER_ONLY_ROOT=from-root\n",
    )
    .unwrap();

    // dotenvy never overwrites already-set vars: most-specific file goes first.
    let env = App::load_env_files("FX_IT_LAYER", [&specific, &root]).unwrap();
    assert_eq!(env.var("KEY").unwrap(), "from-specific");
    assert_eq!(env.var("ONLY_ROOT").unwrap(), "from-root");

    remove("FX_IT_LAYER_KEY");
    remove("FX_IT_LAYER_ONLY_ROOT");
}

#[test]
fn load_env_files_fails_fast_on_missing_path() {
    let dir = tempfile::tempdir().unwrap();
    let existing = dir.path().join("exists.env");
    std::fs::write(&existing, "FX_IT_FF_UNUSED=1\n").unwrap();
    let missing = dir.path().join("does-not-exist.env");

    let err = App::load_env_files("FX_IT_FF", [&existing, &missing])
        .expect_err("missing file must fail fast");
    let msg = err.to_string();
    assert!(
        msg.contains("does-not-exist.env"),
        "error should name the path, was: {msg}"
    );

    remove("FX_IT_FF_UNUSED");
}

#[test]
fn scoped_env_standalone_load() {
    let _g = guard();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".env");
    std::fs::write(&file, "FX_IT_STANDALONE_PORT=9090\n").unwrap();

    let env = ScopedEnv::load("FX_IT_STANDALONE", [&file]).unwrap();
    assert_eq!(env.parse::<u16>("PORT").unwrap(), 9090);

    remove("FX_IT_STANDALONE_PORT");
}

// ---------------------------------------------------------------------------
// Builder .env_file() / .env_files() + App::env_vars()
// ---------------------------------------------------------------------------

#[tokio::test]
async fn builder_env_files_loaded_at_build() {
    let _g = guard();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".env");
    std::fs::write(&file, "FX_IT_BENV_SERVER_PORT=7070\n").unwrap();

    let app = App::builder("EnvFileApp", "FXITBENV")
        .env_prefix("FX_IT_BENV")
        .env_file(&file)
        .startup_banner(false)
        .build()
        .await
        .unwrap();

    let port: u16 = app.env_vars().parse("SERVER_PORT").unwrap();
    assert_eq!(port, 7070);

    remove("FX_IT_BENV_SERVER_PORT");
}

#[tokio::test]
async fn builder_env_files_fail_fast_on_missing() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.env");

    let err = App::builder("MissingEnvApp", "FXITMISS")
        .env_files([&missing])
        .startup_banner(false)
        .build()
        .await
        .expect_err("build() must fail on a missing env file");
    assert!(
        err.to_string().contains("nope.env"),
        "error should name the path, was: {err}"
    );
}

// ---------------------------------------------------------------------------
// env_vars() on builder / App / AppInit
// ---------------------------------------------------------------------------

#[test]
fn builder_env_vars_mid_assembly() {
    let _g = guard();
    set("FX_IT_BLD_TIMEOUT", "45");

    let builder = App::builder("MidAssembly", "FXITBLD").env_prefix("FX_IT_BLD");
    // Non-consuming: readable while the builder is still being assembled.
    let env: ScopedEnv = builder.env_vars();
    assert_eq!(env.parse::<u64>("TIMEOUT").unwrap(), 45);

    remove("FX_IT_BLD_TIMEOUT");
}

#[tokio::test]
async fn app_init_env_vars_via_deref() {
    let _g = guard();
    set("FX_IT_INIT_REGION", "eu-central-1");

    let init = App::builder("InitApp", "FXITINIT")
        .env_prefix("FX_IT_INIT")
        .startup_banner(false)
        .build_init()
        .await
        .unwrap();

    // AppInit derefs to App: env_vars() resolves prefixed keys.
    assert_eq!(init.env_vars().var("REGION").unwrap(), "eu-central-1");

    let app = init.freeze().await.unwrap();
    assert_eq!(app.env_vars().var("REGION").unwrap(), "eu-central-1");

    remove("FX_IT_INIT_REGION");
}

// ---------------------------------------------------------------------------
// .require_env()
// ---------------------------------------------------------------------------

#[tokio::test]
async fn require_env_consolidated_error_lists_all_missing() {
    let _g = guard();
    set("FX_IT_REQ_PRESENT", "yes");

    let err = App::builder("RequireApp", "FXITREQ")
        .env_prefix("FX_IT_REQ")
        .require_env(["PRESENT", "MISSING_ONE", "MISSING_TWO"])
        .startup_banner(false)
        .build()
        .await
        .expect_err("build() must fail on missing required env vars");

    let msg = err.to_string();
    assert!(msg.contains("FX_IT_REQ_MISSING_ONE"), "was: {msg}");
    assert!(msg.contains("FX_IT_REQ_MISSING_TWO"), "was: {msg}");
    assert!(!msg.contains("FX_IT_REQ_PRESENT"), "was: {msg}");

    remove("FX_IT_REQ_PRESENT");
}

#[tokio::test]
async fn require_env_passes_when_all_present() {
    let _g = guard();
    set("FX_IT_REQOK_A", "1");
    set("FX_IT_REQOK_B", "2");

    let app = App::builder("RequireOkApp", "FXITREQOK")
        .env_prefix("FX_IT_REQOK")
        .require_env(["A", "B"])
        .startup_banner(false)
        .build()
        .await
        .unwrap();
    assert_eq!(app.app_name(), "RequireOkApp");

    remove("FX_IT_REQOK_A");
    remove("FX_IT_REQOK_B");
}

// ---------------------------------------------------------------------------
// .config() / .config_file()
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, PartialEq)]
struct ItConfig {
    db_url: String,
    max_connections: u32,
}

#[tokio::test]
async fn config_from_env_var_resolves_via_require() {
    let _g = guard();
    set(
        "FX_IT_CFG_CONFIG",
        r#"{"db_url":"postgres://localhost/it","max_connections":7}"#,
    );

    let app = App::builder("ConfigEnvApp", "FXITCFG")
        .env_prefix("FX_IT_CFG")
        .config::<ItConfig>()
        .startup_banner(false)
        .build()
        .await
        .unwrap();

    let config = app.require::<ItConfig>().unwrap();
    assert_eq!(config.db_url, "postgres://localhost/it");
    assert_eq!(config.max_connections, 7);

    remove("FX_IT_CFG_CONFIG");
}

#[tokio::test]
async fn config_file_resolves_via_require() {
    let dir = tempfile::tempdir().unwrap();
    let path: PathBuf = dir.path().join("config.json");
    std::fs::write(
        &path,
        r#"{"db_url":"postgres://file/it","max_connections":3}"#,
    )
    .unwrap();

    let app = App::builder("ConfigFileApp", "FXITCFGF")
        .config_file::<ItConfig>(&path)
        .startup_banner(false)
        .build()
        .await
        .unwrap();

    let config = app.require::<ItConfig>().unwrap();
    assert_eq!(config.db_url, "postgres://file/it");
    assert_eq!(config.max_connections, 3);
}

#[tokio::test]
async fn config_from_missing_env_var_fails_build() {
    let _g = guard();
    remove("FX_IT_CFGMISS_CONFIG");

    let err = App::builder("ConfigMissApp", "FXITCFGMISS")
        .env_prefix("FX_IT_CFGMISS")
        .config::<ItConfig>()
        .startup_banner(false)
        .build()
        .await
        .expect_err("build() must fail when {PREFIX}_CONFIG is unset");
    assert!(
        err.to_string().contains("FX_IT_CFGMISS_CONFIG"),
        "was: {err}"
    );
}

// ---------------------------------------------------------------------------
// ScopedEnv DI registration
// ---------------------------------------------------------------------------

#[tokio::test]
async fn scoped_env_is_registered_in_di() {
    let _g = guard();
    set("FX_IT_DI_REGION", "us-east-1");

    let app = App::builder("DiApp", "FXITDI")
        .env_prefix("FX_IT_DI")
        .startup_banner(false)
        .build()
        .await
        .unwrap();

    // Resolvable from the container and bound to the same prefix as env_vars().
    let env = app.require::<ScopedEnv>().unwrap();
    assert_eq!(env.prefix(), "FX_IT_DI");
    assert_eq!(env.var("REGION").unwrap(), "us-east-1");
    assert_eq!(&*env, &app.env_vars());

    remove("FX_IT_DI_REGION");
}

#[tokio::test]
async fn scoped_env_di_registration_with_empty_prefix() {
    let app = App::builder("DiNoPrefixApp", "FXITDINP")
        .startup_banner(false)
        .build()
        .await
        .unwrap();

    let env = app.require::<ScopedEnv>().unwrap();
    assert_eq!(env.prefix(), "");
    assert_eq!(&*env, &app.env_vars());
}

// ---------------------------------------------------------------------------
// .env_file() end-to-end bootstrap
// ---------------------------------------------------------------------------

#[tokio::test]
async fn env_files_readable_via_env_vars_after_build() {
    let _g = guard();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".env");
    std::fs::write(
        &file,
        "FX_IT_E2E_SERVER_PORT=8080\nFX_IT_E2E_REGION=eu-west-1\n",
    )
    .unwrap();

    let app = App::builder("E2eApp", "FXITE2E")
        .env_prefix("FX_IT_E2E")
        .env_file(&file)
        .startup_banner(false)
        .build()
        .await
        .unwrap();

    // Recorded env files are loaded at build; reads resolve via prefix.
    let port: u16 = app.env_vars().parse("SERVER_PORT").unwrap();
    assert_eq!(port, 8080);
    assert_eq!(app.env_vars().var("REGION").unwrap(), "eu-west-1");

    // The DI-registered ScopedEnv sees them too.
    let env = app.require::<ScopedEnv>().unwrap();
    assert_eq!(env.parse::<u16>("SERVER_PORT").unwrap(), 8080);

    remove("FX_IT_E2E_SERVER_PORT");
    remove("FX_IT_E2E_REGION");
}

// ---------------------------------------------------------------------------
// .tracing() (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "tracing-setup")]
#[tokio::test]
async fn tracing_config_initializes_subscriber() {
    // Only one test in this binary may initialize the global subscriber.
    let app = App::builder("TracingApp", "FXITTRC")
        .tracing(foxtive::setup::Tracing::default())
        .startup_banner(true)
        .build()
        .await
        .unwrap();
    assert_eq!(app.app_name(), "TracingApp");
}
