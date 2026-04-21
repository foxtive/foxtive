use foxtive_cron::contracts::{InMemoryJobStore, JobStore, JobState};
use std::sync::Arc;
use chrono::{Utc, Duration as ChronoDuration};

mod in_memory_job_store {
    use super::*;

    #[tokio::test]
    async fn new_creates_empty_store() {
        let store = InMemoryJobStore::new();
        let state = store.get_state("nonexistent").await.unwrap();
        assert!(state.is_none());
    }

    #[tokio::test]
    async fn save_state_stores_job_state() {
        let store = InMemoryJobStore::new();
        let state = JobState {
            last_run: Some(Utc::now()),
            consecutive_failures: 2,
            ..Default::default()
        };

        store.save_state("job-1", &state).await.unwrap();
        
        let retrieved = store.get_state("job-1").await.unwrap();
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert!(retrieved.last_run.is_some());
        assert_eq!(retrieved.consecutive_failures, 2);
    }

    #[tokio::test]
    async fn get_state_returns_none_for_missing_job() {
        let store = InMemoryJobStore::new();
        let result = store.get_state("missing-job").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn save_state_updates_existing_state() {
        let store = InMemoryJobStore::new();
        
        // Initial state
        let state = JobState {
            last_run: Some(Utc::now()),
            consecutive_failures: 0,
            ..Default::default()
        };
        store.save_state("job-1", &state).await.unwrap();

        // Updated state
        let updated_state = JobState {
            last_run: Some(Utc::now() + ChronoDuration::hours(1)),
            last_success: Some(Utc::now() + ChronoDuration::hours(1)),
            consecutive_failures: 5,
            ..Default::default()
        };
        store.save_state("job-1", &updated_state).await.unwrap();

        // Verify update
        let retrieved = store.get_state("job-1").await.unwrap().unwrap();
        assert_eq!(retrieved.consecutive_failures, 5);
        assert!(retrieved.last_success.is_some());
    }

    #[tokio::test]
    async fn multiple_jobs_can_be_stored_independently() {
        let store = InMemoryJobStore::new();
        
        let state1 = JobState {
            last_run: Some(Utc::now()),
            ..Default::default()
        };
        store.save_state("job-1", &state1).await.unwrap();

        let state2 = JobState {
            last_failure: Some(Utc::now()),
            consecutive_failures: 3,
            ..Default::default()
        };
        store.save_state("job-2", &state2).await.unwrap();

        let retrieved1 = store.get_state("job-1").await.unwrap().unwrap();
        let retrieved2 = store.get_state("job-2").await.unwrap().unwrap();

        assert!(retrieved1.last_run.is_some());
        assert!(retrieved2.last_failure.is_some());
        assert_eq!(retrieved2.consecutive_failures, 3);
    }

    #[tokio::test]
    async fn job_state_tracks_all_fields() {
        let store = InMemoryJobStore::new();
        
        let now = Utc::now();
        let state = JobState {
            last_run: Some(now),
            last_success: Some(now - ChronoDuration::hours(1)),
            last_failure: Some(now - ChronoDuration::minutes(30)),
            consecutive_failures: 7,
        };
        
        store.save_state("job-1", &state).await.unwrap();
        
        let retrieved = store.get_state("job-1").await.unwrap().unwrap();
        assert_eq!(retrieved.last_run, Some(now));
        assert!(retrieved.last_success.is_some());
        assert!(retrieved.last_failure.is_some());
        assert_eq!(retrieved.consecutive_failures, 7);
    }

    #[tokio::test]
    async fn concurrent_access_is_safe() {
        let store = Arc::new(InMemoryJobStore::new());
        let mut handles = vec![];

        for i in 0..10 {
            let store_clone = store.clone();
            let handle = tokio::spawn(async move {
                let state = JobState {
                    last_run: Some(Utc::now()),
                    ..Default::default()
                };
                store_clone.save_state(&format!("job-{}", i), &state).await.unwrap();
                
                let retrieved = store_clone.get_state(&format!("job-{}", i)).await.unwrap();
                assert!(retrieved.is_some());
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.await.unwrap();
        }
    }
}
