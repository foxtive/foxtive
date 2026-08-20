//! Example: Using foxtive with foxtive-worker for background job processing.
//!
//! Run with:
//! ```shell
//! cargo run --example worker_integration --features "rabbitmq"
//! ```

use std::sync::Arc;

use foxtive::Environment;
use foxtive::prelude::*;

struct EmailJob;

impl EmailJob {
    async fn process(&self, to: &str, subject: &str) -> AppResult<()> {
        println!("Sending email to {to}: {subject}");
        // Simulate email sending
        Ok(())
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    // Build the application container
    let app = App::builder("Worker Integration Demo", "DEMO")
        .environment(Environment::Local)
        .app_key("demo-secret-key")
        .register(EmailJob)
        .build()
        .await?;

    println!("App name: {}", app.app_name());
    println!("Environment: {:?}", app.env());

    let email_job: Arc<EmailJob> = app.require()?;

    // In a real application you would set up a worker here:
    //
    // use foxtive_worker::{Worker, RabbitMqBackend};
    //
    // let worker = Worker::builder()
    //     .backend(RabbitMqBackend::new(app.rabbitmq().clone()))
    //     .handler("email_queue", move |msg| {
    //         let job = email_job.clone();
    //         async move {
    //             job.process(&msg.to, &msg.subject).await?;
    //             Ok(())
    //         }
    //     })
    //     .build();
    //
    // worker.start().await?;

    // Simulate processing a job
    email_job.process("user@example.com", "Welcome!").await?;

    Ok(())
}
