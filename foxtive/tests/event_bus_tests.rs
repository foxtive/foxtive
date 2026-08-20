mod common;

use foxtive::App;
use foxtive::events::{Event, EventHandler};
use foxtive::prelude::AppResult;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Event, Clone, Debug)]
struct TestEvent {
    value: i32,
}

#[derive(Event, Clone, Debug)]
struct AnotherEvent {
    #[allow(unused)]
    message: String,
}

struct CountingHandler {
    count: Arc<AtomicUsize>,
}

impl EventHandler<TestEvent> for CountingHandler {
    async fn handle(&self, _event: &TestEvent, _app: &App) -> AppResult<()> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

struct AnotherCountingHandler {
    count: Arc<AtomicUsize>,
}

impl EventHandler<AnotherEvent> for AnotherCountingHandler {
    async fn handle(&self, _event: &AnotherEvent, _app: &App) -> AppResult<()> {
        self.count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

#[tokio::test]
async fn event_bus_dispatches_to_typed_handler() {
    let count = Arc::new(AtomicUsize::new(0));
    let handler = CountingHandler {
        count: count.clone(),
    };

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.on::<TestEvent, _>(handler);

    assert_eq!(init.events().handler_count::<TestEvent>(), 1);
    assert_eq!(init.events().total_handler_count(), 1);

    let app = init.freeze().await.unwrap();
    app.events()
        .emit(TestEvent { value: 42 }, &app)
        .await
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn event_bus_dispatches_to_closure_handler() {
    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = count.clone();

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.on_event::<TestEvent, _, _>(move |_event, _app| {
        let count = count_clone.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });

    assert_eq!(init.events().handler_count::<TestEvent>(), 1);

    let app = init.freeze().await.unwrap();
    app.events()
        .emit(TestEvent { value: 99 }, &app)
        .await
        .unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn event_bus_dispatches_to_multiple_handlers() {
    let count1 = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::new(AtomicUsize::new(0));

    let handler1 = CountingHandler {
        count: count1.clone(),
    };
    let count2_clone = count2.clone();

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.on::<TestEvent, _>(handler1);
    init.on_event::<TestEvent, _, _>(move |_event, _app| {
        let count = count2_clone.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });

    assert_eq!(init.events().handler_count::<TestEvent>(), 2);
    assert_eq!(init.events().total_handler_count(), 2);

    let app = init.freeze().await.unwrap();
    app.events()
        .emit(TestEvent { value: 1 }, &app)
        .await
        .unwrap();
    assert_eq!(count1.load(Ordering::SeqCst), 1);
    assert_eq!(count2.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn event_bus_no_handlers_returns_ok() {
    let app = App::builder("svc", "SVC").build().await.unwrap();

    assert_eq!(app.events().handler_count::<TestEvent>(), 0);
    assert_eq!(app.events().total_handler_count(), 0);

    let result = app.events().emit(TestEvent { value: 1 }, &app).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn event_bus_different_event_types_are_isolated() {
    let test_count = Arc::new(AtomicUsize::new(0));
    let another_count = Arc::new(AtomicUsize::new(0));

    let test_handler = CountingHandler {
        count: test_count.clone(),
    };
    let another_handler = AnotherCountingHandler {
        count: another_count.clone(),
    };

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.on::<TestEvent, _>(test_handler);
    init.on::<AnotherEvent, _>(another_handler);

    assert_eq!(init.events().handler_count::<TestEvent>(), 1);
    assert_eq!(init.events().handler_count::<AnotherEvent>(), 1);
    assert_eq!(init.events().total_handler_count(), 2);

    let app = init.freeze().await.unwrap();
    app.events()
        .emit(TestEvent { value: 1 }, &app)
        .await
        .unwrap();
    assert_eq!(test_count.load(Ordering::SeqCst), 1);
    assert_eq!(another_count.load(Ordering::SeqCst), 0);

    app.events()
        .emit(
            AnotherEvent {
                message: "hello".into(),
            },
            &app,
        )
        .await
        .unwrap();
    assert_eq!(test_count.load(Ordering::SeqCst), 1);
    assert_eq!(another_count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn event_bus_handler_receives_correct_event_data() {
    let received_value = Arc::new(std::sync::Mutex::new(None));
    let received_clone = received_value.clone();

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();
    init.on_event::<TestEvent, _, _>(move |event, _app| {
        let received = received_clone.clone();
        async move {
            *received.lock().unwrap() = Some(event.value);
            Ok(())
        }
    });

    let app = init.freeze().await.unwrap();
    app.events()
        .emit(TestEvent { value: 123 }, &app)
        .await
        .unwrap();
    assert_eq!(*received_value.lock().unwrap(), Some(123));
}

#[tokio::test]
async fn event_bus_handler_can_access_app() {
    let app_name_received = Arc::new(std::sync::Mutex::new(None));
    let app_name_clone = app_name_received.clone();

    let mut init = App::builder("My Test App", "TEST")
        .build_init()
        .await
        .unwrap();
    init.on_event::<TestEvent, _, _>(move |_event, app| {
        let name = app_name_clone.clone();
        async move {
            *name.lock().unwrap() = Some(app.app_name().to_string());
            Ok(())
        }
    });

    let app = init.freeze().await.unwrap();
    app.events()
        .emit(TestEvent { value: 1 }, &app)
        .await
        .unwrap();
    assert_eq!(
        *app_name_received.lock().unwrap(),
        Some("My Test App".to_string())
    );
}

#[tokio::test]
async fn event_bus_failing_handler_returns_error() {
    let count = Arc::new(AtomicUsize::new(0));
    let count_clone = count.clone();

    let mut init = App::builder("svc", "SVC").build_init().await.unwrap();

    // First handler succeeds
    init.on_event::<TestEvent, _, _>(move |_event, _app| {
        let count = count_clone.clone();
        async move {
            count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });

    // Second handler fails
    init.on_event::<TestEvent, _, _>(|_event, _app| async {
        Err(foxtive::enums::AppMessage::Infrastructure {
            message: "handler failed".into(),
            source: None,
        })
    });

    assert_eq!(init.events().handler_count::<TestEvent>(), 2);

    let app = init.freeze().await.unwrap();
    let result = app.events().emit(TestEvent { value: 1 }, &app).await;
    assert!(result.is_err());

    // First handler still ran despite second failing
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn app_events_accessor_works() {
    let app = App::builder("svc", "SVC").build().await.unwrap();
    let _bus = app.events();
    assert_eq!(app.events().total_handler_count(), 0);
}
