mod common;
use common::*;
use foxtive_supervisor::Supervisor;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[tokio::test]
async fn test_prerequisite_satisfaction() {
    let satisfied = Arc::new(AtomicBool::new(false));
    let s_clone = satisfied.clone();

    let supervisor = Supervisor::new()
        .require("gate", async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            s_clone.store(true, Ordering::SeqCst);
            Ok(())
        })
        .add(MockTask::new("task1"));

    supervisor.start_and_wait_any().await.unwrap();
    assert!(satisfied.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_prerequisite_failure_prevents_startup() {
    let supervisor = Supervisor::new()
        .require("gate", async move { anyhow::bail!("Gate failed") })
        .add(MockTask::new("task1"));

    let result = supervisor.start().await;
    match result {
        Ok(_) => panic!("Supervisor should not have started"),
        Err(e) => assert!(e.to_string().contains("Gate failed")),
    }
}

#[tokio::test]
async fn test_multiple_prerequisites() {
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let c1 = count.clone();
    let c2 = count.clone();

    let supervisor = Supervisor::new()
        .require("gate1", async move {
            c1.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .require("gate2", async move {
            c2.fetch_add(1, Ordering::SeqCst);
            Ok(())
        })
        .add(MockTask::new("task1"));

    supervisor.start_and_wait_any().await.unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
