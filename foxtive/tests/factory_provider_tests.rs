mod common;

use foxtive::lifecycle::ServiceInit;
use foxtive::prelude::*;

// A foreign type (no ServiceInit impl)
struct HttpClient {
    base_url: String,
}

// A service that depends on the foreign type
struct ApiService {
    #[allow(dead_code)]
    url: String,
}

impl ServiceInit for ApiService {
    async fn init(app: &App) -> AppResult<Self> {
        let client = app.require::<HttpClient>()?;
        Ok(Self {
            url: client.base_url.clone(),
        })
    }
}

#[tokio::test]
async fn register_with_simple() {
    let app = App::builder("test", "TST")
        .register_with(|_app| async {
            Ok(HttpClient {
                base_url: "http://example.com".into(),
            })
        })
        .build()
        .await
        .unwrap();

    let client = app.get::<HttpClient>().unwrap();
    assert_eq!(client.base_url, "http://example.com");
}

#[tokio::test]
async fn register_with_accesses_app() {
    struct DepService {
        value: String,
    }

    let app = App::builder("test", "TST")
        .register(DepService {
            value: "dep-value".into(),
        })
        .register_with(|app| {
            let dep = app.get::<DepService>().unwrap();
            let val = dep.value.clone();
            async move {
                Ok(HttpClient {
                    base_url: val,
                })
            }
        })
        .build()
        .await
        .unwrap();

    let client = app.get::<HttpClient>().unwrap();
    assert_eq!(client.base_url, "dep-value");
}

#[tokio::test]
async fn register_with_failing_factory() {
    let result = App::builder("test", "TST")
        .register_with(|_app| async {
            Err::<HttpClient, _>(AppMessage::Infrastructure {
                message: "factory failed".into(),
                source: None,
            })
        })
        .build()
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn register_with_on_app_init() {
    let mut init = App::builder("test", "TST")
        .build_init()
        .await
        .unwrap();

    init.register_with(|_app| async {
        Ok(HttpClient {
            base_url: "http://init.com".into(),
        })
    });

    let app = init.freeze().await.unwrap();
    let client = app.get::<HttpClient>().unwrap();
    assert_eq!(client.base_url, "http://init.com");
}

#[tokio::test]
async fn register_with_foreign_type() {
    // Register a type from an external crate without implementing ServiceInit
    struct ForeignConfig {
        setting: bool,
    }

    let app = App::builder("test", "TST")
        .register_with(|_app| async {
            Ok(ForeignConfig { setting: true })
        })
        .build()
        .await
        .unwrap();

    let config = app.get::<ForeignConfig>().unwrap();
    assert!(config.setting);
}

#[tokio::test]
async fn register_with_participates_in_topo_order() {
    // ApiService depends on HttpClient (registered via register_with)
    let app = App::builder("test", "TST")
        .register_with(|_app| async {
            Ok(HttpClient {
                base_url: "http://ordered.com".into(),
            })
        })
        .register_service::<ApiService>()
        .build()
        .await
        .unwrap();

    let api = app.get::<ApiService>().unwrap();
    assert_eq!(api.url, "http://ordered.com");
}
