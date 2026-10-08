use std::{
    ffi::OsString,
    path::PathBuf,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use vcxsrv_wsl_manager::{
    cli::{Mode, Options},
    controller::Action,
    worker::{ActionQueue, Request},
};
#[test]
fn isolated_paths_are_explicit_absolute_and_singleton_scope_is_not_configurable() {
    let options = Options::parse(
        [
            "doctor",
            "--json",
            "--state-dir",
            r"C:\isolated\private",
            "--config",
            r"C:\isolated\private\config.json",
        ]
        .map(OsString::from),
    )
    .unwrap();
    assert_eq!(options.mode, Mode::Doctor);
    assert_eq!(
        options.state_dir,
        Some(PathBuf::from(r"C:\isolated\private"))
    );
    assert!(Options::parse(["--tray", "--state-dir", "relative"].map(OsString::from)).is_err());
    assert!(Options::parse(["--tray", "--scope", "other"].map(OsString::from)).is_err());
    assert!(Options::parse(["doctor"].map(OsString::from)).is_err());
}
#[test]
fn duplicate_work_is_coalesced_and_exit_cancels_without_waiting() {
    let (queue, receiver) = ActionQueue::new();
    assert!(queue.submit(Action::Launch));
    for _ in 0..1000 {
        assert!(!queue.submit(Action::Launch));
    }
    assert!(matches!(
        receiver.try_recv().unwrap(),
        Request::Action(Action::Launch)
    ));
    assert!(!queue.submit(Action::StartServer));
    queue.complete();
    assert!(queue.submit(Action::StartServer));
    let start = Instant::now();
    queue.shutdown();
    assert!(start.elapsed() < Duration::from_millis(100));
    assert!(queue.cancel().load(Ordering::Acquire));
    assert!(!queue.submit(Action::Launch));
}
#[test]
fn doctor_of_missing_state_is_read_only_and_never_ready() {
    let path = std::env::temp_dir().join(format!("task6-doctor-{}", std::process::id()));
    let mut options = Options::parse([
        OsString::from("doctor"),
        OsString::from("--json"),
        OsString::from("--state-dir"),
        path.clone().into_os_string(),
    ])
    .unwrap();
    options.config = None;
    let snapshot = vcxsrv_wsl_manager::worker::doctor(&options).unwrap();
    assert!(!snapshot.ready);
    assert_eq!(snapshot.state, "WaitingForWsl");
    assert!(!path.exists());
}
#[test]
fn recover_stopped_action_is_explicit_and_does_not_ensure_server_or_guest() {
    let mut controller = vcxsrv_wsl_manager::controller::Controller::new();
    let effects = controller.dispatch(Action::RecoverStoppedRecord);
    assert_eq!(effects.len(), 1);
    assert_eq!(
        effects[0].operation,
        vcxsrv_wsl_manager::controller::Operation::RecoverStoppedRecord
    );
    let effects = controller.observe(vcxsrv_wsl_manager::controller::Event {
        attempt: effects[0].attempt,
        outcome: vcxsrv_wsl_manager::controller::Outcome::Recovered,
    });
    assert!(effects.is_empty());
    assert_eq!(
        controller.state(),
        &vcxsrv_wsl_manager::controller::State::WaitingForWsl
    );
    let options = Options::parse(
        ["recover-stopped", "--state-dir", r"C:\isolated\private"].map(OsString::from),
    )
    .unwrap();
    assert_eq!(options.mode, Mode::Recover);
}
#[test]
fn explicit_missing_config_is_an_error_instead_of_silently_using_defaults() {
    let path = std::env::temp_dir().join(format!("task6-missing-config-{}", std::process::id()));
    let options = Options::parse([
        OsString::from("doctor"),
        OsString::from("--json"),
        OsString::from("--config"),
        path.join("config.json").into_os_string(),
    ])
    .unwrap();
    assert!(vcxsrv_wsl_manager::worker::doctor(&options).is_err());
    assert!(!path.exists());
}
#[test]
fn host_membership_observer_backs_off_and_only_connects_passively_after_guest_appears() {
    use vcxsrv_wsl_manager::controller::State;
    use vcxsrv_wsl_manager::worker::{HostChange, HostPoller};
    let now = Instant::now();
    let mut poller = HostPoller::new_at(now);
    assert!(!poller.due_at(now + Duration::from_secs(4)));
    assert!(poller.due_at(now + Duration::from_secs(5)));
    assert_eq!(
        poller.observe_at(false, &State::WaitingForWsl, now + Duration::from_secs(5)),
        HostChange::None
    );
    assert_eq!(
        poller.observe_at(false, &State::WaitingForWsl, now + Duration::from_secs(10)),
        HostChange::None
    );
    assert!(!poller.due_at(now + Duration::from_secs(19)));
    assert!(poller.due_at(now + Duration::from_secs(20)));
    assert_eq!(
        poller.observe_at(false, &State::WaitingForWsl, now + Duration::from_secs(20)),
        HostChange::None
    );
    assert!(!poller.due_at(now + Duration::from_secs(39)));
    assert!(poller.due_at(now + Duration::from_secs(40)));
    poller.observe_at(false, &State::WaitingForWsl, now + Duration::from_secs(40));
    assert!(!poller.due_at(now + Duration::from_secs(69)));
    assert!(poller.due_at(now + Duration::from_secs(70)));
    assert_eq!(
        poller.observe_at(true, &State::WaitingForWsl, now + Duration::from_secs(70)),
        HostChange::ConnectPassively
    );
    assert_eq!(
        poller.observe_at(true, &State::Ready, now + Duration::from_secs(75)),
        HostChange::None
    );
    assert_eq!(
        poller.observe_at(false, &State::Ready, now + Duration::from_secs(85)),
        HostChange::InvalidateGuest
    );
    let (queue, receiver) = ActionQueue::new();
    assert!(queue.submit(Action::Launch));
    assert!(!queue.begin_observation());
    receiver.try_recv().unwrap();
    queue.complete();
    assert!(queue.begin_observation());
    assert!(!queue.submit(Action::Launch));
}
