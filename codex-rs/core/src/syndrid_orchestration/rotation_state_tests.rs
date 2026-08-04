use super::super::account_pools::AccountPoolMember;
use super::super::account_pools::AccountPoolProviderFamily;
use super::super::account_pools::AccountPoolSelectionPolicy;
use super::super::account_pools::AccountPoolTarget;
use super::super::account_pools::NamedAccountPool;
use super::super::account_pools::NamedAccountPoolRegistry;
use super::super::account_pools::PoolId;
use super::super::account_pools::PoolMemberId;
use super::super::codex_accounts::CodexAccountProfileId;
use super::super::routing_profiles::RoutingRole;
use super::AccountPoolRotationState;
use super::PoolRotationError;
use pretty_assertions::assert_eq;

fn pool(ids: &[&str]) -> NamedAccountPool {
    NamedAccountPool {
        id: PoolId::new("pool-a").unwrap(),
        display_name: "Pool".to_string(),
        provider_family: AccountPoolProviderFamily::NativeCodex,
        members: ids
            .iter()
            .map(|id| AccountPoolMember {
                id: PoolMemberId::new(*id).unwrap(),
                target: AccountPoolTarget::native_codex(
                    CodexAccountProfileId::new(format!("account-{id}")).unwrap(),
                ),
            })
            .collect(),
        selection_policy: AccountPoolSelectionPolicy::RoundRobin,
    }
}

fn registry(ids: &[&str]) -> NamedAccountPoolRegistry {
    let mut registry = NamedAccountPoolRegistry::default();
    registry.insert(pool(ids)).unwrap();
    registry
}

fn reserve(
    state: &mut AccountPoolRotationState,
    registry: &NamedAccountPoolRegistry,
    role: RoutingRole,
) -> super::PoolSelectionReservation {
    state
        .reserve_next_member(registry, &PoolId::new("pool-a").unwrap(), role)
        .unwrap()
}

#[test]
fn canonical_order_is_independent_of_insertion_order() {
    let first = registry(&["member-c", "member-a", "member-b"]);
    let second = registry(&["member-b", "member-c", "member-a"]);
    let mut first_state = AccountPoolRotationState::new();
    let mut second_state = AccountPoolRotationState::new();
    let first_role = RoutingRole::Planner;
    let second_role = RoutingRole::Planner;

    for expected in ["member-a", "member-b", "member-c"] {
        let mut first_reservation = reserve(&mut first_state, &first, first_role);
        let mut second_reservation = reserve(&mut second_state, &second, second_role);
        assert_eq!(first_reservation.member_id().as_str(), expected);
        assert_eq!(second_reservation.member_id().as_str(), expected);
        first_reservation.commit(&mut first_state, &first).unwrap();
        second_reservation
            .commit(&mut second_state, &second)
            .unwrap();
    }
}

#[test]
fn abort_repeats_selection_and_commit_advances_with_wraparound() {
    let registry = registry(&["a", "b", "c"]);
    let mut state = AccountPoolRotationState::new();
    let pool_id = PoolId::new("pool-a").unwrap();
    let aborted = reserve(&mut state, &registry, RoutingRole::Planner);
    assert_eq!(aborted.member_id().as_str(), "a");
    aborted.abort();

    for expected in ["a", "b", "c", "a"] {
        let mut reservation = reserve(&mut state, &registry, RoutingRole::Planner);
        assert_eq!(reservation.member_id().as_str(), expected);
        reservation.commit(&mut state, &registry).unwrap();
    }
    assert_eq!(
        state.cursor_generation(&pool_id, RoutingRole::Planner),
        Some(4)
    );
}

#[test]
fn roles_and_pools_have_independent_cursors() {
    let mut registry = registry(&["a", "b"]);
    let mut other = pool(&["a", "b"]);
    other.id = PoolId::new("pool-b").unwrap();
    registry.insert(other).unwrap();
    let mut state = AccountPoolRotationState::new();

    let mut planner = reserve(&mut state, &registry, RoutingRole::Planner);
    planner.commit(&mut state, &registry).unwrap();
    let mut executor = reserve(&mut state, &registry, RoutingRole::Executor);
    assert_eq!(executor.member_id().as_str(), "a");
    executor.commit(&mut state, &registry).unwrap();
    let mut other_pool = state
        .reserve_next_member(
            &registry,
            &PoolId::new("pool-b").unwrap(),
            RoutingRole::Planner,
        )
        .unwrap();
    assert_eq!(other_pool.member_id().as_str(), "a");
    other_pool.commit(&mut state, &registry).unwrap();
}

#[test]
fn duplicate_and_stale_commits_do_not_advance_twice() {
    let registry = registry(&["a", "b"]);
    let mut state = AccountPoolRotationState::new();
    let mut first = reserve(&mut state, &registry, RoutingRole::Planner);
    let mut second = reserve(&mut state, &registry, RoutingRole::Planner);
    first.commit(&mut state, &registry).unwrap();
    assert_eq!(
        first.commit(&mut state, &registry),
        Err(PoolRotationError::ReservationAlreadyCommitted)
    );
    assert_eq!(
        second.commit(&mut state, &registry),
        Err(PoolRotationError::StaleReservation)
    );
    assert_eq!(
        reserve(&mut state, &registry, RoutingRole::Planner)
            .member_id()
            .as_str(),
        "b"
    );
}

#[test]
fn cloned_reservations_cannot_both_commit() {
    let registry = registry(&["a", "b"]);
    let mut state = AccountPoolRotationState::new();
    let mut original = reserve(&mut state, &registry, RoutingRole::Planner);
    let mut clone = original.clone();

    original.commit(&mut state, &registry).unwrap();
    assert_eq!(
        clone.commit(&mut state, &registry),
        Err(PoolRotationError::StaleReservation)
    );
    assert_eq!(
        reserve(&mut state, &registry, RoutingRole::Planner)
            .member_id()
            .as_str(),
        "b"
    );
}

#[test]
fn changed_pool_rejects_old_reservation_without_advancing() {
    let mut registry = registry(&["a", "b"]);
    let mut state = AccountPoolRotationState::new();
    let mut reservation = reserve(&mut state, &registry, RoutingRole::Planner);

    let mut changed = registry
        .get(&PoolId::new("pool-a").unwrap())
        .unwrap()
        .clone();
    changed.members.push(AccountPoolMember {
        id: PoolMemberId::new("c").unwrap(),
        target: AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-c").unwrap()),
    });
    registry.remove(&PoolId::new("pool-a").unwrap());
    registry.insert(changed).unwrap();

    assert_eq!(
        reservation.commit(&mut state, &registry),
        Err(PoolRotationError::PoolFingerprintMismatch)
    );
    assert_eq!(
        reserve(&mut state, &registry, RoutingRole::Planner)
            .member_id()
            .as_str(),
        "a"
    );
}

#[test]
fn pool_changes_reset_to_first_but_display_name_changes_do_not() {
    let mut registry = registry(&["a", "b"]);
    let mut state = AccountPoolRotationState::new();
    let mut first = reserve(&mut state, &registry, RoutingRole::Planner);
    first.commit(&mut state, &registry).unwrap();

    let mut renamed = registry
        .get(&PoolId::new("pool-a").unwrap())
        .unwrap()
        .clone();
    renamed.display_name = "Renamed".to_string();
    registry.remove(&PoolId::new("pool-a").unwrap());
    registry.insert(renamed).unwrap();
    assert_eq!(
        reserve(&mut state, &registry, RoutingRole::Planner)
            .member_id()
            .as_str(),
        "b"
    );

    let mut changed = registry
        .get(&PoolId::new("pool-a").unwrap())
        .unwrap()
        .clone();
    changed.members.push(AccountPoolMember {
        id: PoolMemberId::new("c").unwrap(),
        target: AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-c").unwrap()),
    });
    registry.remove(&PoolId::new("pool-a").unwrap());
    registry.insert(changed).unwrap();
    assert_eq!(
        reserve(&mut state, &registry, RoutingRole::Planner)
            .member_id()
            .as_str(),
        "a"
    );

    let mut target_changed = registry
        .get(&PoolId::new("pool-a").unwrap())
        .unwrap()
        .clone();
    target_changed.members[0].target =
        AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-renamed").unwrap());
    registry.remove(&PoolId::new("pool-a").unwrap());
    registry.insert(target_changed).unwrap();
    assert_eq!(
        reserve(&mut state, &registry, RoutingRole::Planner)
            .member_id()
            .as_str(),
        "a"
    );

    let mut policy_changed = registry
        .get(&PoolId::new("pool-a").unwrap())
        .unwrap()
        .clone();
    policy_changed.selection_policy =
        AccountPoolSelectionPolicy::ExplicitMember(PoolMemberId::new("a").unwrap());
    registry.remove(&PoolId::new("pool-a").unwrap());
    registry.insert(policy_changed).unwrap();
    assert_eq!(
        state.reserve_next_member(
            &registry,
            &PoolId::new("pool-a").unwrap(),
            RoutingRole::Planner
        ),
        Err(PoolRotationError::UnsupportedPolicy)
    );
}

#[test]
fn unavailable_configured_member_is_not_skipped() {
    let registry = registry(&["a", "b"]);
    let mut state = AccountPoolRotationState::new();
    let mut first = reserve(&mut state, &registry, RoutingRole::Planner);
    assert_eq!(first.member_id().as_str(), "a");
    first.commit(&mut state, &registry).unwrap();
    let next = reserve(&mut state, &registry, RoutingRole::Planner);
    assert_eq!(next.member_id().as_str(), "b");
    next.abort();
    assert_eq!(
        reserve(&mut state, &registry, RoutingRole::Planner)
            .member_id()
            .as_str(),
        "b"
    );
}

#[test]
fn explicit_member_does_not_use_rotation_state() {
    let mut registry = registry(&["a", "b"]);
    let mut explicit = registry
        .get(&PoolId::new("pool-a").unwrap())
        .unwrap()
        .clone();
    explicit.selection_policy =
        AccountPoolSelectionPolicy::ExplicitMember(PoolMemberId::new("b").unwrap());
    registry.remove(&PoolId::new("pool-a").unwrap());
    registry.insert(explicit).unwrap();
    let mut state = AccountPoolRotationState::new();
    assert_eq!(
        state.reserve_next_member(
            &registry,
            &PoolId::new("pool-a").unwrap(),
            RoutingRole::Planner
        ),
        Err(PoolRotationError::UnsupportedPolicy)
    );
    assert_eq!(
        state.cursor_generation(&PoolId::new("pool-a").unwrap(), RoutingRole::Planner),
        None
    );
}

#[test]
fn one_member_round_robin_repeats_exactly() {
    let registry = registry(&["only"]);
    let mut state = AccountPoolRotationState::new();
    for _ in 0..3 {
        let mut reservation = reserve(&mut state, &registry, RoutingRole::Planner);
        assert_eq!(reservation.member_id().as_str(), "only");
        reservation.commit(&mut state, &registry).unwrap();
    }
}
