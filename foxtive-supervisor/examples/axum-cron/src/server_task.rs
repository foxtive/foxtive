use axum::response::Html;
use axum::routing::get;
use axum::Router;
use foxtive_supervisor::contracts::SupervisedTask;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tracing::{info, warn};

pub struct HttpServerTask {
    addr: String,
    shutdown_tx: broadcast::Sender<()>,
}

impl HttpServerTask {
    pub fn create(addr: &str) -> HttpServerTask {
        let (shutdown_tx, _) = broadcast::channel(1);
        HttpServerTask {
            addr: addr.to_string(),
            shutdown_tx,
        }
    }
}

#[async_trait::async_trait]
impl SupervisedTask for HttpServerTask {
    fn id(&self) -> &'static str {
        "server-task"
    }

    fn name(&self) -> String {
        "axum-server-task".to_string()
    }

    async fn run(&self) -> anyhow::Result<()> {
        info!("Starting HTTP server on {}", self.addr);

        async fn handler() -> Html<&'static str> {
            Html("<h1>Hello, World!</h1>")
        }

        let app = Router::new().route("/", get(handler));

        let listener = TcpListener::bind(&self.addr).await?;

        // Create a shutdown receiver for this run
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        // Serve with graceful shutdown
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                shutdown_rx.recv().await.ok();
                info!("Axum server received shutdown signal");
            })
            .await?;

        Ok(())
    }

    async fn should_restart(&self, _attempt: usize, error: &str) -> bool {
        // Don't restart if port is in use
        !error.contains("address already in use")
    }

    async fn on_shutdown(&self) {
        warn!("Shutting down HTTP server");
        // Send shutdown signal to axum
        let _ = self.shutdown_tx.send(());
    }
}
