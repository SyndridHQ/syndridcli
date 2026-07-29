use codex_core::ProductionCancellationReason;
use codex_core::ProductionOrchestrationLifecycle;

use super::ProductionOrchestrationCancellationRegistration;

#[tokio::test]
async fn matching_turn_interrupt_requests_cancellation() {
    let mut lifecycle = ProductionOrchestrationLifecycle::<(), &'static str>::spawn(
        "turn-1",
        |_cancellation| async { Ok(()) },
    );
    let handle = lifecycle.cancellation_handle();
    let registration =
        ProductionOrchestrationCancellationRegistration::new("turn-1", handle.clone());

    assert!(registration.request_cancel("turn-1", ProductionCancellationReason::User));
    assert!(handle.is_cancelled());
    let _ = lifecycle.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn wrong_turn_does_not_cancel_registration() {
    let mut lifecycle = ProductionOrchestrationLifecycle::<(), &'static str>::spawn(
        "turn-2",
        |_cancellation| async { Ok(()) },
    );
    let handle = lifecycle.cancellation_handle();
    let registration =
        ProductionOrchestrationCancellationRegistration::new("turn-2", handle.clone());

    assert!(!registration.request_cancel("other-turn", ProductionCancellationReason::User));
    assert!(!handle.is_cancelled());
    let _ = lifecycle.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn duplicate_interrupt_is_harmless() {
    let mut lifecycle = ProductionOrchestrationLifecycle::<(), &'static str>::spawn(
        "turn-3",
        |_cancellation| async { Ok(()) },
    );
    let handle = lifecycle.cancellation_handle();
    let registration = ProductionOrchestrationCancellationRegistration::new("turn-3", handle);

    assert!(registration.request_cancel("turn-3", ProductionCancellationReason::User));
    assert!(!registration.request_cancel("turn-3", ProductionCancellationReason::User));
    let _ = lifecycle.shutdown(std::time::Duration::from_secs(1)).await;
}

#[tokio::test]
async fn absent_registration_has_no_effect() {
    let mut lifecycle = ProductionOrchestrationLifecycle::<(), &'static str>::spawn(
        "turn-4",
        |_cancellation| async { Ok(()) },
    );
    let handle = lifecycle.cancellation_handle();
    assert!(!handle.is_cancelled());
    let _ = lifecycle.shutdown(std::time::Duration::from_secs(1)).await;
}
