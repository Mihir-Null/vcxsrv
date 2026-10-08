use std::time::{Duration, Instant};
use vcxsrv_wsl_manager::controller::*;
use vcxsrv_wsl_manager::diagnostics::Failure;
fn reply(c: &mut Controller, e: &Effect, outcome: Outcome) -> Vec<Effect> {
    c.observe(Event {
        attempt: e.attempt,
        outcome,
    })
}
#[test]
fn passive_start_never_wakes_stopped_guest_and_coalesces_clicks() {
    let mut c = Controller::new();
    let e = c.dispatch(Action::StartServer);
    assert_eq!(e[0].operation, Operation::EnsureServer);
    assert!(c.dispatch(Action::StartServer).is_empty());
    let e = reply(&mut c, &e[0], Outcome::ServerOwned);
    assert_eq!(e[0].operation, Operation::DiscoverRunning);
    assert!(reply(&mut c, &e[0], Outcome::Running(false)).is_empty());
    assert_eq!(c.state(), &State::WaitingForWsl);
}
#[test]
fn launch_can_wake_only_via_configured_guest_effect_and_requires_authentication() {
    let mut c = Controller::new();
    let e = c.dispatch(Action::Launch);
    let e = reply(&mut c, &e[0], Outcome::ServerOwned);
    let e = reply(&mut c, &e[0], Outcome::Running(false));
    assert_eq!(e[0].operation, Operation::Bootstrap { allow_start: true });
    let e = reply(&mut c, &e[0], Outcome::Bootstrapped);
    assert_eq!(c.state(), &State::Connecting);
    assert_eq!(e[0].operation, Operation::Probe);
    let e = reply(&mut c, &e[0], Outcome::Authenticated);
    assert_eq!(c.state(), &State::Ready);
    assert_eq!(e[0].operation, Operation::Launch);
    assert!(reply(&mut c, &e[0], Outcome::Launched).is_empty());
    assert_eq!(c.state(), &State::Ready);
}
#[test]
fn refresh_never_bootstraps_and_ready_has_no_polling() {
    let mut c = Controller::new();
    let e = c.dispatch(Action::Refresh);
    assert_eq!(e[0].operation, Operation::EnsureServer);
    let e = reply(&mut c, &e[0], Outcome::ServerOwned);
    let e = reply(&mut c, &e[0], Outcome::Running(true));
    assert_eq!(e[0].operation, Operation::Probe);
    assert!(reply(&mut c, &e[0], Outcome::Authenticated).is_empty());
    assert_eq!(c.state(), &State::Ready);
    c.invalidate(Invalidation::VmChanged);
    assert_eq!(c.state(), &State::WaitingForWsl);
}
#[test]
fn deadline_is_shared_and_stale_or_out_of_order_results_cannot_establish_ready() {
    let mut c = Controller::new();
    let e = c.dispatch(Action::Launch);
    let first = e[0];
    assert!(reply(&mut c, &first, Outcome::Authenticated).is_empty());
    assert_eq!(c.state(), &State::Connecting);
    let e = reply(&mut c, &first, Outcome::ServerOwned);
    assert_eq!(first.deadline, e[0].deadline);
    assert!(first.remaining_at(first.deadline).is_err());
    assert!(first.deadline.duration_since(Instant::now()) <= Duration::from_secs(15));
    c.dispatch(Action::Detach);
    assert!(reply(&mut c, &e[0], Outcome::Running(true)).is_empty());
    assert_ne!(c.state(), &State::Ready);
}
#[test]
fn retry_does_not_extend_deadline_and_confirmed_stop_is_explicit() {
    let mut c = Controller::new();
    let e = c.dispatch(Action::StartServer);
    let e = reply(&mut c, &e[0], Outcome::ServerOwned);
    let e = reply(&mut c, &e[0], Outcome::Running(true));
    let e = reply(&mut c, &e[0], Outcome::Bootstrapped);
    let retry = reply(&mut c, &e[0], Outcome::Retryable);
    assert_eq!(retry[0].deadline, e[0].deadline);
    assert_eq!(retry[0].operation, Operation::Probe);
    let stop = c.dispatch(Action::StopServerConfirmed);
    assert_eq!(stop[0].operation, Operation::StopOwnedServer);
    assert!(c.dispatch(Action::StopServerConfirmed).is_empty());
    assert!(reply(&mut c, &retry[0], Outcome::Authenticated).is_empty());
    reply(&mut c, &stop[0], Outcome::Stopped);
    assert_eq!(c.state(), &State::WaitingForWsl);
}
#[test]
fn shutdown_races_and_credential_conflicts_are_safe_errors() {
    let mut c = Controller::new();
    let e = c.dispatch(Action::Launch);
    reply(&mut c, &e[0], Outcome::Failed(Failure::CredentialConflict));
    assert!(matches!(c.state(), State::Error(_)));
}

#[test]
fn late_success_at_deadline_is_an_error_and_vm_change_invalidates_attempt() {
    let mut c = Controller::new();
    let e = c.dispatch(Action::Launch)[0];
    assert!(c
        .observe_at(
            Event {
                attempt: e.attempt,
                outcome: Outcome::ServerOwned
            },
            e.deadline
        )
        .is_empty());
    assert!(matches!(c.state(), State::Error(_)));
    let e = c.dispatch(Action::Launch)[0];
    c.invalidate(Invalidation::VmChanged);
    assert!(reply(&mut c, &e, Outcome::ServerOwned).is_empty());
    assert_eq!(c.state(), &State::WaitingForWsl);
}
