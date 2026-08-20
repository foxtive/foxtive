//! Integration tests for complex DI dependency resolution scenarios.
//!
//! Covers the two-phase construction system:
//! - Phase 1a: DFS-based order resolution from declared deps
//! - Phase 1b: Sequential construction; `DependencyMissing` failures deferred
//! - Phase 2: Single retry pass for undeclared runtime deps
//! - Phase 3: Lazy<T> wiring
//!
//! These tests verify that services are correctly constructed regardless of
//! registration order, and that the retry mechanism handles undeclared deps.

mod common;

use foxtive::App;
use foxtive::enums::AppMessage;
use foxtive::lifecycle::{Service, ServiceInit};
use foxtive::prelude::AppResult;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// Reversed registration order (manual ServiceInit, no dependencies())

struct LeafService {
    value: &'static str,
}

impl ServiceInit for LeafService {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { value: "leaf" })
    }
    // No dependencies() override - defaults to vec![]
}

struct MiddleService {
    leaf_value: String,
}

impl ServiceInit for MiddleService {
    async fn init(app: &App) -> AppResult<Self> {
        let leaf = app.require::<LeafService>()?;
        Ok(Self {
            leaf_value: leaf.value.to_string(),
        })
    }
    // No dependencies() override - undeclared runtime dep on LeafService
}

struct RootService {
    middle_value: String,
}

impl ServiceInit for RootService {
    async fn init(app: &App) -> AppResult<Self> {
        let middle = app.require::<MiddleService>()?;
        Ok(Self {
            middle_value: middle.leaf_value.clone(),
        })
    }
    // No dependencies() override - undeclared runtime dep on MiddleService
}

/// Services registered in reverse order (root first, leaf last).
/// Without Phase 2 retry, this would fail because RootService is constructed
/// before its deps. With retry, all services eventually resolve.
#[tokio::test]
async fn reversed_registration_order_with_undeclared_deps() {
    let app = App::builder("dep-test", "DEP")
        .register_service::<RootService>()
        .register_service::<MiddleService>()
        .register_service::<LeafService>()
        .build()
        .await
        .unwrap();

    let root = app.get::<RootService>().unwrap();
    assert_eq!(root.middle_value, "leaf");

    let middle = app.get::<MiddleService>().unwrap();
    assert_eq!(middle.leaf_value, "leaf");

    let leaf = app.get::<LeafService>().unwrap();
    assert_eq!(leaf.value, "leaf");
}

// Deep chain registered completely reversed: A → B → C → D → E

struct ChainE;
impl ServiceInit for ChainE {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self)
    }
}

struct ChainD;
impl ServiceInit for ChainD {
    async fn init(app: &App) -> AppResult<Self> {
        let _e = app.require::<ChainE>()?;
        Ok(Self)
    }
}

struct ChainC;
impl ServiceInit for ChainC {
    async fn init(app: &App) -> AppResult<Self> {
        let _d = app.require::<ChainD>()?;
        Ok(Self)
    }
}

struct ChainB;
impl ServiceInit for ChainB {
    async fn init(app: &App) -> AppResult<Self> {
        let _c = app.require::<ChainC>()?;
        Ok(Self)
    }
}

struct ChainA;
impl ServiceInit for ChainA {
    async fn init(app: &App) -> AppResult<Self> {
        let _b = app.require::<ChainB>()?;
        Ok(Self)
    }
}

#[tokio::test]
async fn deep_chain_reversed_registration() {
    let app = App::builder("dep-test", "DEP")
        .register_service::<ChainA>()
        .register_service::<ChainB>()
        .register_service::<ChainC>()
        .register_service::<ChainD>()
        .register_service::<ChainE>()
        .build()
        .await
        .unwrap();

    assert!(app.get::<ChainA>().is_some());
    assert!(app.get::<ChainB>().is_some());
    assert!(app.get::<ChainC>().is_some());
    assert!(app.get::<ChainD>().is_some());
    assert!(app.get::<ChainE>().is_some());
}

// Diamond dependency with manual impls

struct DiamondBase {
    id: usize,
}

impl ServiceInit for DiamondBase {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { id: 42 })
    }
}

struct DiamondLeft {
    base_id: usize,
}

impl ServiceInit for DiamondLeft {
    async fn init(app: &App) -> AppResult<Self> {
        let base = app.require::<DiamondBase>()?;
        Ok(Self { base_id: base.id })
    }
}

struct DiamondRight {
    base_id: usize,
}

impl ServiceInit for DiamondRight {
    async fn init(app: &App) -> AppResult<Self> {
        let base = app.require::<DiamondBase>()?;
        Ok(Self { base_id: base.id })
    }
}

struct DiamondTop {
    left_base_id: usize,
    right_base_id: usize,
}

impl ServiceInit for DiamondTop {
    async fn init(app: &App) -> AppResult<Self> {
        let left = app.require::<DiamondLeft>()?;
        let right = app.require::<DiamondRight>()?;
        Ok(Self {
            left_base_id: left.base_id,
            right_base_id: right.base_id,
        })
    }
}

#[tokio::test]
async fn diamond_dependency_reversed_registration() {
    // Register in worst possible order: top first, base last
    let app = App::builder("dep-test", "DEP")
        .register_service::<DiamondTop>()
        .register_service::<DiamondLeft>()
        .register_service::<DiamondRight>()
        .register_service::<DiamondBase>()
        .build()
        .await
        .unwrap();

    let top = app.get::<DiamondTop>().unwrap();
    assert_eq!(top.left_base_id, 42);
    assert_eq!(top.right_base_id, 42);
}

// Shuffled #[derive(Service)] with declared deps

#[derive(Default)]
struct DeclaredLeafManual {
    value: usize,
}

impl ServiceInit for DeclaredLeafManual {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { value: 99 })
    }
}

#[derive(Service, Default)]
struct DeclaredMiddleManual {
    #[dependency]
    leaf: Arc<DeclaredLeafManual>,
}

#[derive(Service, Default)]
struct DeclaredRootManual {
    #[dependency]
    middle: Arc<DeclaredMiddleManual>,
}

#[tokio::test]
async fn derived_services_shuffled_registration() {
    // Register root first - DFS should still resolve leaf → middle → root
    let app = App::builder("dep-test", "DEP")
        .register_service::<DeclaredRootManual>()
        .register_service::<DeclaredLeafManual>()
        .register_service::<DeclaredMiddleManual>()
        .build()
        .await
        .unwrap();

    let root = app.get::<DeclaredRootManual>().unwrap();
    assert_eq!(root.middle.leaf.value, 99);
}

// Phase 2 deadlock: mutual undeclared deps

struct DeadlockA;
impl ServiceInit for DeadlockA {
    async fn init(app: &App) -> AppResult<Self> {
        let _b = app.require::<DeadlockB>()?;
        Ok(Self)
    }
}

struct DeadlockB;
impl ServiceInit for DeadlockB {
    async fn init(app: &App) -> AppResult<Self> {
        let _a = app.require::<DeadlockA>()?;
        Ok(Self)
    }
}

#[tokio::test]
async fn phase2_deadlock_produces_clear_error() {
    let result = App::builder("dep-test", "DEP")
        .register_service::<DeadlockA>()
        .register_service::<DeadlockB>()
        .build()
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("DI0001")
            || err.to_lowercase().contains("deadlock")
            || err.contains("not yet constructed"),
        "Expected DI error, got: {err}"
    );
}

// Terminal error fails immediately (no retry)

static TERMINAL_INIT_COUNT: AtomicUsize = AtomicUsize::new(0);

struct TerminalFailService;

impl ServiceInit for TerminalFailService {
    async fn init(_app: &App) -> AppResult<Self> {
        TERMINAL_INIT_COUNT.fetch_add(1, Ordering::SeqCst);
        Err(AppMessage::Infrastructure {
            message: "database connection refused".into(),
            source: None,
        })
    }
}

struct InnocentBystander;
impl ServiceInit for InnocentBystander {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self)
    }
}

#[tokio::test]
async fn terminal_error_not_retried() {
    TERMINAL_INIT_COUNT.store(0, Ordering::SeqCst);

    let result = App::builder("dep-test", "DEP")
        .register_service::<InnocentBystander>()
        .register_service::<TerminalFailService>()
        .build()
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("database connection refused"));

    // init() should have been called exactly once - no retries
    assert_eq!(TERMINAL_INIT_COUNT.load(Ordering::SeqCst), 1);
}

// Mixed declared and undeclared deps

struct DeclaredDep;
impl ServiceInit for DeclaredDep {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self)
    }
}

struct UndeclaredDep;
impl ServiceInit for UndeclaredDep {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self)
    }
}

struct MixedConsumer {
    has_declared: bool,
    has_undeclared: bool,
}

impl ServiceInit for MixedConsumer {
    async fn init(app: &App) -> AppResult<Self> {
        let _declared = app.require::<DeclaredDep>()?;
        let _undeclared = app.require::<UndeclaredDep>()?;
        Ok(Self {
            has_declared: true,
            has_undeclared: true,
        })
    }

    fn dependencies() -> Vec<&'static str> {
        // Only declare DeclaredDep - UndeclaredDep is a runtime surprise
        vec![std::any::type_name::<DeclaredDep>()]
    }
}

#[tokio::test]
async fn mixed_declared_and_undeclared_deps() {
    // Register consumer first, then deps in reverse
    let app = App::builder("dep-test", "DEP")
        .register_service::<MixedConsumer>()
        .register_service::<UndeclaredDep>()
        .register_service::<DeclaredDep>()
        .build()
        .await
        .unwrap();

    let consumer = app.get::<MixedConsumer>().unwrap();
    assert!(consumer.has_declared);
    assert!(consumer.has_undeclared);
}

// Interleaved register() and register_service()

struct EagerlyRegistered {
    data: String,
}

struct FactoryConsumer {
    data: String,
}

impl ServiceInit for FactoryConsumer {
    async fn init(app: &App) -> AppResult<Self> {
        let eager = app.require::<EagerlyRegistered>()?;
        Ok(Self {
            data: eager.data.clone(),
        })
    }
}

#[tokio::test]
async fn interleaved_register_and_service() {
    let app = App::builder("dep-test", "DEP")
        .register_service::<FactoryConsumer>()
        .register(EagerlyRegistered {
            data: "pre-registered".into(),
        })
        .build()
        .await
        .unwrap();

    let consumer = app.get::<FactoryConsumer>().unwrap();
    assert_eq!(consumer.data, "pre-registered");
}

// Mutable service with undeclared deps (Phase 2 retry)

struct MutableDep {
    count: u32,
}

impl ServiceInit for MutableDep {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self { count: 7 })
    }
}

struct MutableConsumer {
    dep_count: u32,
}

impl ServiceInit for MutableConsumer {
    async fn init(app: &App) -> AppResult<Self> {
        let dep = app.require::<MutableDep>()?;
        Ok(Self {
            dep_count: dep.count,
        })
    }
    // No dependencies() - will need Phase 2 retry
}

#[tokio::test]
async fn mutable_service_retry_with_undeclared_deps() {
    let app = App::builder("dep-test", "DEP")
        .register_mutable_service::<MutableConsumer>()
        .register_service::<MutableDep>()
        .build()
        .await
        .unwrap();

    let consumer = app.get_mutable::<MutableConsumer>().unwrap();
    assert_eq!(consumer.read().dep_count, 7);
}

// Construction order verification with DFS

static DFS_ORDER: AtomicUsize = AtomicUsize::new(0);

struct TrackerBase {
    constructed_at: usize,
}

impl ServiceInit for TrackerBase {
    async fn init(_app: &App) -> AppResult<Self> {
        let at = DFS_ORDER.fetch_add(1, Ordering::SeqCst);
        Ok(Self { constructed_at: at })
    }
}

struct TrackerLeft {
    constructed_at: usize,
}

impl ServiceInit for TrackerLeft {
    async fn init(app: &App) -> AppResult<Self> {
        let _base = app.require::<TrackerBase>()?;
        let at = DFS_ORDER.fetch_add(1, Ordering::SeqCst);
        Ok(Self { constructed_at: at })
    }
    fn dependencies() -> Vec<&'static str> {
        vec![std::any::type_name::<TrackerBase>()]
    }
}

struct TrackerRight {
    constructed_at: usize,
}

impl ServiceInit for TrackerRight {
    async fn init(app: &App) -> AppResult<Self> {
        let _base = app.require::<TrackerBase>()?;
        let at = DFS_ORDER.fetch_add(1, Ordering::SeqCst);
        Ok(Self { constructed_at: at })
    }
    fn dependencies() -> Vec<&'static str> {
        vec![std::any::type_name::<TrackerBase>()]
    }
}

#[tokio::test]
async fn dfs_order_respects_declared_deps() {
    DFS_ORDER.store(0, Ordering::SeqCst);

    // Register in reverse: left, right, base
    // DFS should construct base first (it's a declared dep of both)
    let app = App::builder("dep-test", "DEP")
        .register_service::<TrackerLeft>()
        .register_service::<TrackerRight>()
        .register_service::<TrackerBase>()
        .build()
        .await
        .unwrap();

    let base = app.get::<TrackerBase>().unwrap();
    let left = app.get::<TrackerLeft>().unwrap();
    let right = app.get::<TrackerRight>().unwrap();

    // Base must be constructed before both left and right
    assert!(
        base.constructed_at < left.constructed_at,
        "Base ({}) should be constructed before Left ({})",
        base.constructed_at,
        left.constructed_at
    );
    assert!(
        base.constructed_at < right.constructed_at,
        "Base ({}) should be constructed before Right ({})",
        base.constructed_at,
        right.constructed_at
    );
}

// Never-registered dependency produces clear error

struct NeedsMissing;
impl ServiceInit for NeedsMissing {
    async fn init(app: &App) -> AppResult<Self> {
        let _ghost = app.require::<NonExistentDep>()?;
        Ok(Self)
    }
}

struct NonExistentDep;

#[tokio::test]
async fn never_registered_dep_produces_clear_error() {
    let result = App::builder("dep-test", "DEP")
        .register_service::<NeedsMissing>()
        .build()
        .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    // Should mention the missing type, DI0001 code, or deadlock
    assert!(
        err.contains("DI0001")
            || err.contains("not registered")
            || err.contains("deadlock")
            || err.contains("not yet constructed"),
        "Expected clear error about missing dep, got: {err}"
    );
}

// Multiple independent chains

struct IndepA1;
impl ServiceInit for IndepA1 {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self)
    }
}

struct IndepA2;
impl ServiceInit for IndepA2 {
    async fn init(app: &App) -> AppResult<Self> {
        let _a1 = app.require::<IndepA1>()?;
        Ok(Self)
    }
    fn dependencies() -> Vec<&'static str> {
        vec![std::any::type_name::<IndepA1>()]
    }
}

struct IndepB1;
impl ServiceInit for IndepB1 {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self)
    }
}

struct IndepB2;
impl ServiceInit for IndepB2 {
    async fn init(app: &App) -> AppResult<Self> {
        let _b1 = app.require::<IndepB1>()?;
        Ok(Self)
    }
    fn dependencies() -> Vec<&'static str> {
        vec![std::any::type_name::<IndepB1>()]
    }
}

#[tokio::test]
async fn multiple_independent_chains_interleaved() {
    // Interleave: A2, B2, A1, B1 - worst case for naive ordering
    let app = App::builder("dep-test", "DEP")
        .register_service::<IndepA2>()
        .register_service::<IndepB2>()
        .register_service::<IndepA1>()
        .register_service::<IndepB1>()
        .build()
        .await
        .unwrap();

    assert!(app.get::<IndepA1>().is_some());
    assert!(app.get::<IndepA2>().is_some());
    assert!(app.get::<IndepB1>().is_some());
    assert!(app.get::<IndepB2>().is_some());
}

// AppInit::register_service with reversed undeclared deps

struct InitPhaseLeaf;
impl ServiceInit for InitPhaseLeaf {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self)
    }
}

struct InitPhaseRoot;
impl ServiceInit for InitPhaseRoot {
    async fn init(app: &App) -> AppResult<Self> {
        let _leaf = app.require::<InitPhaseLeaf>()?;
        Ok(Self)
    }
    // No dependencies() - undeclared dep
}

#[tokio::test]
async fn init_register_service_reversed_undeclared() {
    let mut init = App::builder("dep-test", "DEP").build_init().await.unwrap();

    // Register root before leaf via AppInit
    init.register_service::<InitPhaseRoot>();
    init.register_service::<InitPhaseLeaf>();

    let app = init.freeze().await.unwrap();
    assert!(app.get::<InitPhaseRoot>().is_some());
    assert!(app.get::<InitPhaseLeaf>().is_some());
}

// Service depending on pre-registered + factory-constructed

struct PreReg {
    val: i32,
}

struct FactoryBuilt;
impl ServiceInit for FactoryBuilt {
    async fn init(_app: &App) -> AppResult<Self> {
        Ok(Self)
    }
}

struct HybridConsumer {
    pre_val: i32,
    has_factory: bool,
}

impl ServiceInit for HybridConsumer {
    async fn init(app: &App) -> AppResult<Self> {
        let pre = app.require::<PreReg>()?;
        let _factory = app.require::<FactoryBuilt>()?;
        Ok(Self {
            pre_val: pre.val,
            has_factory: true,
        })
    }
    fn dependencies() -> Vec<&'static str> {
        // Only declare FactoryBuilt - PreReg is pre-registered
        vec![std::any::type_name::<FactoryBuilt>()]
    }
}

#[tokio::test]
async fn hybrid_pre_registered_and_factory_built() {
    let app = App::builder("dep-test", "DEP")
        .register_service::<HybridConsumer>()
        .register_service::<FactoryBuilt>()
        .register(PreReg { val: 123 })
        .build()
        .await
        .unwrap();

    let consumer = app.get::<HybridConsumer>().unwrap();
    assert_eq!(consumer.pre_val, 123);
    assert!(consumer.has_factory);
}
