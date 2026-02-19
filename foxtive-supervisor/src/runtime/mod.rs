//! Task runtime module for the Foxtive supervisor
//!
//! This module provides the core runtime functionality for supervising tasks,
//! including task registration, dependency management, prerequisite handling,
//! and supervision loops.

// Re-export public types and functions
pub use core::TaskRuntime;
pub use helpers::{spawn_supervised, spawn_supervised_many};
pub use types::{PrerequisiteFuture, SupervisionResult, TaskEntry};

// Internal modules
mod core;
mod helpers;
mod supervision;
mod types;
mod validation;

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_supervision() {
        use crate::contracts::SupervisedTask;

        struct TestTask;

        #[async_trait::async_trait]
        impl SupervisedTask for TestTask {
            fn id(&self) -> &'static str {
                "test"
            }
            async fn run(&self) -> anyhow::Result<()> {
                Ok(())
            }
        }

        let handle = spawn_supervised(TestTask);
        let result = handle.await.unwrap();
        assert_eq!(
            result.final_status,
            crate::enums::SupervisionStatus::CompletedNormally
        );
    }
}
