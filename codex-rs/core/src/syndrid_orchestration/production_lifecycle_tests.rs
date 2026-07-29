use std::time::Duration;

use super::ProductionCancellationReason;
use super::ProductionOrchestrationLifecycle;
use super::ProductionOrchestrationLifecycleError;
use super::ProductionOrchestrationLifecycleState;

#[tokio::test]
async fn cancellation_reaches_coordinator_and_is_idempotent() {
    let mut lifecycle =
        ProductionOrchestrationLifecycle::spawn("run-1", |cancellation| async move {
            cancellation.cancelled().await;
            Err::<(), _>("cancelled")
        });
    assert!(lifecycle.request_cancel(ProductionCancellationReason::User));
    assert!(!lifecycle.request_cancel(ProductionCancellationReason::Timeout));
    assert_eq!(
        lifecycle.cancellation_handle().cancellation_reason(),
        Some(ProductionCancellationReason::User)
    );
    assert_eq!(
        lifecycle.complete().await,
        Err(ProductionOrchestrationLifecycleError::Coordinator(
            "cancelled"
        ))
    );
}

#[tokio::test]
async fn successful_coordinator_is_joined() {
    let mut lifecycle = ProductionOrchestrationLifecycle::spawn("run-2", |_cancellation| async {
        Ok::<_, &'static str>(42)
    });
    assert_eq!(lifecycle.complete().await, Ok(42));
    assert_eq!(
        lifecycle.state(),
        ProductionOrchestrationLifecycleState::Completed
    );
}

#[tokio::test]
async fn coordinator_failure_is_preserved() {
    let mut lifecycle = ProductionOrchestrationLifecycle::spawn("run-3", |_cancellation| async {
        Err::<(), _>("provider failure")
    });
    assert_eq!(
        lifecycle.complete().await,
        Err(ProductionOrchestrationLifecycleError::Coordinator(
            "provider failure"
        ))
    );
}

#[tokio::test]
async fn coordinator_panic_becomes_join_failure() {
    let mut lifecycle = ProductionOrchestrationLifecycle::<(), &'static str>::spawn(
        "run-4",
        |_cancellation| async { panic!("test panic") },
    );
    assert_eq!(
        lifecycle.complete().await,
        Err(ProductionOrchestrationLifecycleError::CoordinatorJoinFailure)
    );
}

#[tokio::test]
async fn observation_bridge_is_joined_after_coordinator() {
    let mut lifecycle = ProductionOrchestrationLifecycle::spawn("run-5", |_cancellation| async {
        Ok::<_, &'static str>(())
    });
    let (bridge_done_tx, bridge_done_rx) = tokio::sync::oneshot::channel();
    lifecycle.attach_observation_bridge(tokio::spawn(async move {
        let _ = bridge_done_tx.send(());
    }));
    assert_eq!(lifecycle.complete().await, Ok(()));
    assert!(bridge_done_rx.await.is_ok());
}

#[tokio::test]
async fn shutdown_is_bounded_and_aborts_owned_tasks() {
    let mut lifecycle = ProductionOrchestrationLifecycle::<(), &'static str>::spawn(
        "run-6",
        |_cancellation| async {
            std::future::pending::<()>().await;
            Ok(())
        },
    );
    let result = lifecycle.shutdown(Duration::from_millis(10)).await;
    assert_eq!(
        result,
        Err(ProductionOrchestrationLifecycleError::ShutdownTimedOut)
    );
    assert_eq!(
        lifecycle.state(),
        ProductionOrchestrationLifecycleState::ShutdownTimedOut
    );
}

#[tokio::test]
async fn session_shutdown_reason_is_distinct_from_user_cancellation() {
    let mut lifecycle =
        ProductionOrchestrationLifecycle::spawn("run-7", |cancellation| async move {
            cancellation.cancelled().await;
            Err::<(), _>("stopped")
        });
    let handle = lifecycle.cancellation_handle();
    let _ = lifecycle.shutdown(Duration::from_secs(1)).await;
    assert_eq!(
        handle.cancellation_reason(),
        Some(ProductionCancellationReason::SessionShutdown)
    );
}

#[tokio::test]
async fn timeout_reason_is_distinct_from_user_cancellation() {
    let mut lifecycle = ProductionOrchestrationLifecycle::<(), &'static str>::spawn(
        "run-8",
        |cancellation| async move {
            cancellation.cancelled().await;
            Err("timed out")
        },
    );
    let handle = lifecycle.cancellation_handle();
    assert!(lifecycle.request_cancel(ProductionCancellationReason::Timeout));
    assert_eq!(
        lifecycle.complete().await,
        Err(ProductionOrchestrationLifecycleError::Coordinator(
            "timed out"
        ))
    );
    assert_eq!(
        handle.cancellation_reason(),
        Some(ProductionCancellationReason::Timeout)
    );
}
