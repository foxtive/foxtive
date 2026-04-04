mod common;
use foxtive_supervisor::{
    Supervisor,
    enums::{BackoffStrategy, RestartPolicy},
};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[tokio::test]
#[cfg(feature = "cron")]
async fn test_cron_scheduled_task() {
    struct CronTask {
        run_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for CronTask {
        fn id(&self) -> &'static str { "cron-task" }
        
        fn cron_schedule(&self) -> Option<&'static str> {
            // Run every second
            Some("*/1 * * * * * *")
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let run_count = Arc::new(AtomicUsize::new(0));
    let supervisor = Supervisor::new().add(CronTask { 
        run_count: run_count.clone() 
    });

    let runtime = supervisor.start().await.unwrap();
    
    // Wait for 2.5 seconds to allow at least 2 executions
    tokio::time::sleep(Duration::from_millis(2500)).await;
    
    runtime.shutdown().await;
    
    // Should have run at least twice (at 0s and 1s marks)
    let count = run_count.load(Ordering::SeqCst);
    assert!(count >= 2, "Expected at least 2 runs, got {}", count);
}

#[tokio::test]
#[cfg(feature = "cron")]
async fn test_cron_with_immediate_execution() {
    struct ImmediateCronTask {
        run_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ImmediateCronTask {
        fn id(&self) -> &'static str { "immediate-cron" }
        
        fn cron_schedule(&self) -> Option<&'static str> {
            // Run every 2 seconds
            Some("*/2 * * * * * *")
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let run_count = Arc::new(AtomicUsize::new(0));
    let supervisor = Supervisor::new().add(ImmediateCronTask { 
        run_count: run_count.clone() 
    });

    let start = Instant::now();
    let runtime = supervisor.start().await.unwrap();
    
    // Wait for 2.5 seconds to ensure at least one execution (cron runs every 2s)
    tokio::time::sleep(Duration::from_millis(2500)).await;
    
    runtime.shutdown().await;
    let elapsed = start.elapsed();
    
    // Should have run at least once
    let count = run_count.load(Ordering::SeqCst);
    assert!(count >= 1, "Expected at least 1 run, got {}", count);
    assert!(elapsed < Duration::from_secs(3), "Test took too long: {:?}", elapsed);
}

#[tokio::test]
#[cfg(feature = "cron")]
async fn test_cron_task_can_be_stopped() {
    struct StoppableCronTask {
        run_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for StoppableCronTask {
        fn id(&self) -> &'static str { "stoppable-cron" }
        
        fn cron_schedule(&self) -> Option<&'static str> {
            // Run every second
            Some("*/1 * * * * * *")
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let run_count = Arc::new(AtomicUsize::new(0));
    let supervisor = Supervisor::new().add(StoppableCronTask { 
        run_count: run_count.clone() 
    });

    let runtime = supervisor.start().await.unwrap();
    
    // Let it run briefly
    tokio::time::sleep(Duration::from_millis(500)).await;
    let count_before_stop = run_count.load(Ordering::SeqCst);
    
    // Shutdown should stop the task even if it's waiting for next schedule
    runtime.shutdown().await;
    
    // Wait a bit more to ensure no more runs happen
    tokio::time::sleep(Duration::from_millis(1500)).await;
    
    let count_after_stop = run_count.load(Ordering::SeqCst);
    
    // Allow for at most 1 additional execution during shutdown (race condition)
    // The important thing is that it doesn't continue running indefinitely
    assert!(
        count_after_stop <= count_before_stop + 1,
        "Task continued running after shutdown (before: {}, after: {})",
        count_before_stop,
        count_after_stop
    );
}

#[tokio::test]
async fn test_initial_delay() {
    use std::time::Instant;
    
    struct DelayedTask {
        first_run_time: Arc<tokio::sync::Mutex<Option<Instant>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for DelayedTask {
        fn id(&self) -> &'static str { "delayed-task" }
        
        fn initial_delay(&self) -> Option<Duration> {
            Some(Duration::from_millis(500))
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            let mut first_run = self.first_run_time.lock().await;
            if first_run.is_none() {
                *first_run = Some(Instant::now());
            }
            Ok(())
        }
    }

    let first_run_time = Arc::new(tokio::sync::Mutex::new(None));
    let start = Instant::now();
    
    let supervisor = Supervisor::new().add(DelayedTask { 
        first_run_time: first_run_time.clone() 
    });

    let runtime = supervisor.start().await.unwrap();
    
    // Wait for the task to run
    tokio::time::sleep(Duration::from_millis(1000)).await;
    
    runtime.shutdown().await;
    
    let elapsed = start.elapsed();
    let first_run = first_run_time.lock().await;
    
    assert!(first_run.is_some(), "Task should have run");
    let time_to_first_run = first_run.unwrap().duration_since(start);
    
    // The first run should have happened after at least 500ms delay
    assert!(time_to_first_run >= Duration::from_millis(450), 
            "First run was too early: {:?}", time_to_first_run);
    assert!(elapsed >= Duration::from_millis(950), 
            "Total elapsed time too short: {:?}", elapsed);
}

#[tokio::test]
async fn test_initial_delay_with_jitter() {
    use std::time::Instant;
    
    struct JitteredTask {
        first_run_time: Arc<tokio::sync::Mutex<Option<Instant>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for JitteredTask {
        fn id(&self) -> &'static str { "jittered-task" }
        
        fn initial_delay(&self) -> Option<Duration> {
            Some(Duration::from_millis(500))
        }
        
        fn jitter(&self) -> Option<(Duration, Duration)> {
            // Add 0-200ms of jitter
            Some((Duration::from_millis(0), Duration::from_millis(200)))
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            let mut first_run = self.first_run_time.lock().await;
            if first_run.is_none() {
                *first_run = Some(Instant::now());
            }
            Ok(())
        }
    }

    let first_run_time = Arc::new(tokio::sync::Mutex::new(None));
    let start = Instant::now();
    
    let supervisor = Supervisor::new().add(JitteredTask { 
        first_run_time: first_run_time.clone() 
    });

    let runtime = supervisor.start().await.unwrap();
    
    // Wait for the task to run (base delay 500ms + max jitter 200ms = 700ms)
    tokio::time::sleep(Duration::from_millis(1000)).await;
    
    runtime.shutdown().await;
    
    let _elapsed = start.elapsed();
    let first_run = first_run_time.lock().await;
    
    assert!(first_run.is_some(), "Task should have run");
    let time_to_first_run = first_run.unwrap().duration_since(start);
    
    // The first run should have happened after at least 500ms (base delay)
    assert!(time_to_first_run >= Duration::from_millis(450), 
            "First run was too early: {:?}", time_to_first_run);
    // And at most 700ms (base + max jitter) plus some tolerance
    assert!(time_to_first_run <= Duration::from_millis(800), 
            "First run was too late (jitter may have been excessive): {:?}", time_to_first_run);
}

#[tokio::test]
async fn test_rate_limiting_restart_interval() {
    use std::time::Instant;
    
    struct RateLimitedTask {
        run_times: Arc<tokio::sync::Mutex<Vec<Instant>>>,
        fail_count: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for RateLimitedTask {
        fn id(&self) -> &'static str { "rate-limited-task" }
        
        fn backoff_strategy(&self) -> foxtive_supervisor::enums::BackoffStrategy {
            // Use zero backoff so we can test rate limiting in isolation
            foxtive_supervisor::enums::BackoffStrategy::Fixed(Duration::from_millis(0))
        }
        
        fn min_restart_interval(&self) -> Option<Duration> {
            // Minimum 500ms between restarts
            Some(Duration::from_millis(500))
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            let now = Instant::now();
            self.run_times.lock().await.push(now);
            
            // Fail first 2 times to trigger restarts
            let count = self.fail_count.fetch_add(1, Ordering::SeqCst);
            if count < 2 {
                anyhow::bail!("Intentional failure {}", count);
            }
            Ok(())
        }
    }

    let run_times = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let fail_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    
    let supervisor = Supervisor::new().add(RateLimitedTask { 
        run_times: run_times.clone(),
        fail_count: fail_count.clone(),
    });

    let runtime = supervisor.start().await.unwrap();
    
    // Wait for task to complete (should take at least 1 second due to rate limiting)
    tokio::time::sleep(Duration::from_millis(3000)).await;
    
    runtime.shutdown().await;
    
    let times = run_times.lock().await;
    assert_eq!(times.len(), 3, "Should have run 3 times (2 failures + 1 success)");
    
    // Check that time between first and second run is at least 500ms
    let interval_1 = times[1].duration_since(times[0]);
    assert!(interval_1 >= Duration::from_millis(450), 
            "First restart interval too short: {:?} (should be >= 500ms)", interval_1);
    
    // Check that time between second and third run is at least 500ms
    let interval_2 = times[2].duration_since(times[1]);
    assert!(interval_2 >= Duration::from_millis(450), 
            "Second restart interval too short: {:?} (should be >= 500ms)", interval_2);
}

#[tokio::test]
async fn test_time_window_configuration() {
    // This test verifies that the time window configuration is accepted and doesn't cause errors
    // Testing actual time window enforcement would require waiting hours, so we just verify setup
    
    struct TimeWindowTask {
        run_count: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for TimeWindowTask {
        fn id(&self) -> &'static str { "time-window-task" }
        
        fn execution_time_window(&self) -> Option<(Option<u8>, Option<u8>)> {
            // Only run between 9 AM and 5 PM UTC
            Some((Some(9), Some(17)))
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let run_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let supervisor = Supervisor::new().add(TimeWindowTask { 
        run_count: run_count.clone() 
    });

    let runtime = supervisor.start().await.unwrap();
    
    // Give it a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    runtime.shutdown().await;
    
    // Task may or may not have run depending on current time
    // The important thing is that it didn't crash during setup
    let _count = run_count.load(Ordering::SeqCst);
}

#[tokio::test]
#[cfg(feature = "cron")]
async fn test_cron_with_invalid_expression() {
    struct InvalidCronTask;

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for InvalidCronTask {
        fn id(&self) -> &'static str { "invalid-cron" }
        
        fn cron_schedule(&self) -> Option<&'static str> {
            // Invalid cron expression - should be handled gracefully
            Some("* * *")  // Too few fields
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    let supervisor = Supervisor::new().add(InvalidCronTask);
    
    // Should still start, but task may not execute
    let runtime = supervisor.start().await.unwrap();
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    runtime.shutdown().await;
}

#[tokio::test]
#[cfg(feature = "cron")]
async fn test_multiple_cron_tasks_different_schedules() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    struct MultiCronTask {
        id: &'static str,
        run_count: Arc<AtomicUsize>,
        schedule: &'static str,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for MultiCronTask {
        fn id(&self) -> &'static str { self.id }
        
        fn cron_schedule(&self) -> Option<&'static str> {
            Some(self.schedule)
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    
    let count1 = Arc::new(AtomicUsize::new(0));
    let count2 = Arc::new(AtomicUsize::new(0));
    
    let supervisor = Supervisor::new()
        .add(MultiCronTask {
            id: "fast-cron",
            run_count: count1.clone(),
            schedule: "*/1 * * * * * *",  // Every second
        })
        .add(MultiCronTask {
            id: "slow-cron",
            run_count: count2.clone(),
            schedule: "*/2 * * * * * *",  // Every 2 seconds
        });
    
    let runtime = supervisor.start().await.unwrap();
    
    // Wait for 3 seconds
    tokio::time::sleep(Duration::from_millis(3500)).await;
    
    runtime.shutdown().await;
    
    let c1 = count1.load(Ordering::SeqCst);
    let c2 = count2.load(Ordering::SeqCst);
    
    // Fast cron should have run at least as often as slow cron (with some tolerance for timing)
    // Allow up to 20% variance due to scheduling jitter
    let ratio = c1 as f64 / c2.max(1) as f64;
    assert!(ratio >= 0.8, 
            "Fast cron ({} runs) should run at least ~80% as often as slow cron ({} runs), ratio: {:.2}", 
            c1, c2, ratio);
    assert!(c1 >= 2, "Fast cron should have run at least twice");
    assert!(c2 >= 1, "Slow cron should have run at least once");
}

#[tokio::test]
async fn test_initial_delay_zero_duration() {
    struct ZeroDelayTask {
        run_count: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for ZeroDelayTask {
        fn id(&self) -> &'static str { "zero-delay" }
        
        fn initial_delay(&self) -> Option<Duration> {
            Some(Duration::ZERO)
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    let run_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let supervisor = Supervisor::new().add(ZeroDelayTask { 
        run_count: run_count.clone() 
    });

    let runtime = supervisor.start().await.unwrap();
    
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    runtime.shutdown().await;
    
    // Should run immediately despite zero delay
    assert!(run_count.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
#[cfg(feature = "cron")]
async fn test_cron_task_with_very_frequent_schedule() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    struct FrequentCronTask {
        run_count: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for FrequentCronTask {
        fn id(&self) -> &'static str { "frequent-cron" }
        
        fn cron_schedule(&self) -> Option<&'static str> {
            // Run every 100ms (sub-second scheduling)
            Some("*/100 * * * * * *")
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            self.run_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }
    
    let run_count = Arc::new(AtomicUsize::new(0));
    let supervisor = Supervisor::new().add(FrequentCronTask { 
        run_count: run_count.clone() 
    });
    
    let runtime = supervisor.start().await.unwrap();
    
    // Wait for 1 second - should run ~10 times
    tokio::time::sleep(Duration::from_millis(1100)).await;
    
    runtime.shutdown().await;
    
    let count = run_count.load(Ordering::SeqCst);
    assert!(count >= 5, "Frequent cron should have run multiple times, got {}", count);
}

#[tokio::test]
async fn test_combined_initial_delay_and_restart_backoff() {
    use std::sync::Arc;
    use std::time::Instant;
    
    struct CombinedDelayTask {
        run_times: Arc<tokio::sync::Mutex<Vec<Instant>>>,
    }

    #[async_trait::async_trait]
    impl foxtive_supervisor::contracts::SupervisedTask for CombinedDelayTask {
        fn id(&self) -> &'static str { "combined-delay" }
        
        fn initial_delay(&self) -> Option<Duration> {
            Some(Duration::from_millis(200))
        }
        
        fn restart_policy(&self) -> RestartPolicy {
            RestartPolicy::MaxAttempts(3)
        }
        
        fn backoff_strategy(&self) -> BackoffStrategy {
            BackoffStrategy::Fixed(Duration::from_millis(100))
        }
        
        async fn run(&self) -> anyhow::Result<()> {
            let mut times = self.run_times.lock().await;
            times.push(Instant::now());
            Err(anyhow::anyhow!("Intentional failure"))
        }
    }
    
    let run_times = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let start = Instant::now();
    
    let supervisor = Supervisor::new().add(CombinedDelayTask { 
        run_times: run_times.clone() 
    });
    
    let runtime = supervisor.start().await.unwrap();
    
    // Wait for all attempts to complete
    tokio::time::sleep(Duration::from_millis(800)).await;
    
    runtime.shutdown().await;
    
    let times = run_times.lock().await;
    assert_eq!(times.len(), 3, "Should have attempted 3 times");
    
    // First run should be after initial delay
    let first_run_delay = times[0].duration_since(start);
    assert!(first_run_delay >= Duration::from_millis(180), 
            "First run should respect initial delay: {:?}", first_run_delay);
}
