mod common;

use foxtive::container::Lazy;
use foxtive::lifecycle::Service;
use std::sync::Arc;

// #[service(all)] - all fields treated as deps

#[derive(Service)]
struct AllDepService {
    dep_a: Arc<ServiceA>,
    dep_b: Arc<ServiceB>,
}

#[derive(Service, Default)]
struct ServiceA;

#[derive(Service, Default)]
struct ServiceB;

#[tokio::test]
async fn service_all_resolves_all_fields() {
    let app = foxtive::App::builder("all-test", "ALLT")
        .register_service::<ServiceA>()
        .register_service::<ServiceB>()
        .register_service::<AllDepService>()
        .build()
        .await
        .unwrap();

    let svc = app.require::<AllDepService>().unwrap();
    let _a = &*svc.dep_a;
    let _b = &*svc.dep_b;
}

// #[service(all)] with #[default] opt-out

#[derive(Default)]
struct LocalState {
    counter: u64,
}

#[derive(Service)]
#[service(all)]
struct MixedService {
    dep_a: Arc<ServiceA>,
    #[foxtive(default)]
    state: LocalState,
    dep_b: Arc<ServiceB>,
}

#[tokio::test]
async fn service_all_with_default_opt_out() {
    let app = foxtive::App::builder("all-test", "ALLT")
        .register_service::<ServiceA>()
        .register_service::<ServiceB>()
        .register_service::<MixedService>()
        .build()
        .await
        .unwrap();

    let svc = app.require::<MixedService>().unwrap();
    // Dependencies resolved
    let _a = &*svc.dep_a;
    let _b = &*svc.dep_b;
    // #[default] field got Default::default()
    assert_eq!(svc.state.counter, 0);
}

// #[service(all)] with Lazy<T> fields

#[derive(Service)]
#[service(all)]
struct LazyAllService {
    dep_a: Arc<ServiceA>,
    lazy_b: Lazy<ServiceB>,
}

#[tokio::test]
async fn service_all_with_lazy_fields() {
    let app = foxtive::App::builder("all-test", "ALLT")
        .register_service::<ServiceA>()
        .register_service::<ServiceB>()
        .register_service::<LazyAllService>()
        .build()
        .await
        .unwrap();

    let svc = app.require::<LazyAllService>().unwrap();
    let _a = &*svc.dep_a;
    assert!(svc.lazy_b.is_filled());
    let _b = &*svc.lazy_b;
}

// #[service(all, mutable)] combined

#[derive(Service)]
#[service(all, mutable)]
struct MutableAllService {
    dep_a: Arc<ServiceA>,
}

#[tokio::test]
async fn service_all_with_mutable() {
    let app = foxtive::App::builder("all-test", "ALLT")
        .register_service::<ServiceA>()
        .register_service::<MutableAllService>()
        .build()
        .await
        .unwrap();

    let svc = app.require_mutable::<MutableAllService>().unwrap();
    let _a = &*svc.read().dep_a;
}

// Backwards compat: no #[service(all)] still uses opt-in

#[derive(Service, Default)]
struct OptInService {
    #[dependency]
    dep_a: Arc<ServiceA>,
    #[foxtive(default)]
    state: LocalState,
}

#[tokio::test]
async fn backwards_compat_opt_in_still_works() {
    let app = foxtive::App::builder("all-test", "ALLT")
        .register_service::<ServiceA>()
        .register_service::<OptInService>()
        .build()
        .await
        .unwrap();

    let svc = app.require::<OptInService>().unwrap();
    let _a = &*svc.dep_a;
    assert_eq!(svc.state.counter, 0);
}
