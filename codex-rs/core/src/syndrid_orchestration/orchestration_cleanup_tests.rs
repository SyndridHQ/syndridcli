use super::orchestration_cleanup::CleanupChildKind;
use super::orchestration_cleanup::OrchestrationCleanup;
use std::sync::Arc;

#[test]
fn cleanup_tracks_children_and_reservations_before_completion() {
    let cleanup = OrchestrationCleanup::new(7);
    let child = cleanup
        .register_child(7, CleanupChildKind::Planner)
        .expect("child registration");
    let provider = cleanup
        .register_provider_reservation(7)
        .expect("provider reservation");
    let tool = cleanup
        .register_tool_reservation(7)
        .expect("tool reservation");
    cleanup.begin(7).expect("cleanup request");
    let snapshot = cleanup.snapshot(7).expect("snapshot");
    assert_eq!(snapshot.active_children, 1);
    assert_eq!(snapshot.unresolved_provider_reservations, 1);
    assert_eq!(snapshot.unresolved_tool_reservations, 1);
    assert!(cleanup.complete(7).is_err());
    cleanup.complete_child(7, child).expect("child completion");
    cleanup
        .complete_child(7, child)
        .expect("duplicate completion");
    cleanup
        .resolve_provider_reservation(7, provider)
        .expect("provider resolution");
    cleanup
        .resolve_tool_reservation(7, tool)
        .expect("tool resolution");
    cleanup
        .resolve_tool_reservation(7, tool)
        .expect("duplicate tool resolution");
    cleanup.complete(7).expect("cleanup completion");
    assert!(
        cleanup
            .snapshot(7)
            .expect("final snapshot")
            .ready_for_terminalization()
    );
    assert!(
        cleanup
            .register_child(7, CleanupChildKind::Verifier)
            .is_err()
    );
    assert!(cleanup.register_provider_reservation(8).is_err());
}

#[test]
fn dropped_guards_release_children_and_reservations_on_early_exit() {
    let cleanup = Arc::new(OrchestrationCleanup::new(11));
    {
        let _child = cleanup
            .register_child_guard(11, CleanupChildKind::ExecutorBatch)
            .expect("child registration");
        let _provider = cleanup
            .register_provider_reservation_guard(11)
            .expect("provider reservation");
        let _tool = cleanup
            .register_tool_reservation_guard(11)
            .expect("tool reservation");
        let snapshot = cleanup.snapshot(11).expect("snapshot");
        assert_eq!(snapshot.active_children, 1);
        assert_eq!(snapshot.unresolved_provider_reservations, 1);
        assert_eq!(snapshot.unresolved_tool_reservations, 1);
    }

    cleanup.begin(11).expect("cleanup request");
    cleanup.complete(11).expect("cleanup completion");
    assert!(
        cleanup
            .snapshot(11)
            .expect("final snapshot")
            .ready_for_terminalization()
    );
}

#[test]
fn explicit_guard_completion_is_idempotent() {
    let cleanup = Arc::new(OrchestrationCleanup::new(13));
    let mut child = cleanup
        .register_child_guard(13, CleanupChildKind::Provider)
        .expect("child registration");
    let mut reservation = cleanup
        .register_provider_reservation_guard(13)
        .expect("provider reservation");

    child.complete().expect("child completion");
    child.complete().expect("duplicate child completion");
    reservation.resolve().expect("reservation resolution");
    reservation
        .resolve()
        .expect("duplicate reservation resolution");
    cleanup.begin(13).expect("cleanup request");
    cleanup.complete(13).expect("cleanup completion");
}
