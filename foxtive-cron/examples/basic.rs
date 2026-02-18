use foxtive_cron::{Cron, CronResult};

#[tokio::main]
async fn main() {
    let mut cron = Cron::new();

    // Async function
    cron.add_job_fn(
        "async-hello-job",  // stable id
        "Inline Hello Job", // human-readable name
        "*/1 * * * * * *",  // every second
        async_runner,
    )
    .expect("Failed to add job");

    // Blocking function
    cron.add_blocking_job_fn(
        "heavy-task",      // stable id
        "Heavy Task",      // human-readable name
        "*/2 * * * * * *", // every 2 seconds
        blocking_runner,
    )
    .expect("Failed to add job");

    cron.run().await;
}

async fn async_runner() -> CronResult<()> {
    println!("Hello from async fn job at {}", chrono::Utc::now());
    Ok(())
}

fn blocking_runner() -> CronResult<()> {
    println!("Hello from blocking fn job at {}", chrono::Utc::now());
    Ok(())
}
