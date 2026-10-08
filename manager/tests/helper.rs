use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use vcxsrv_wsl_manager::{
    config::Config,
    helper::{HelperClient, HelperError, HelperReply},
    process::{CommandSpec, ProcessError, ProcessRunner},
};
#[test]
fn helper_reply_requires_exact_bounded_schema_and_authenticated_probe() {
    let good = br#"{"version":1,"verb":"probe","result":"ready","display":1,"port":106001}"#;
    assert_eq!(
        HelperReply::parse(good, "probe", 1).unwrap(),
        HelperReply::Ready
    );
    for bad in [br#"{"version":1,"verb":"bootstrap","result":"ready","display":1,"port":106001}"#.as_slice(),br#"{"version":1,"verb":"probe","result":"ready","display":0,"port":106001}"#,br#"{"version":1,"verb":"probe","result":"ready","display":1,"port":106001,"secret":"cookie"}"#,br#"{"version":2,"verb":"probe","result":"ready","display":1,"port":106001}"#] {
  assert!(HelperReply::parse(bad,"probe",1).is_err());
 }
    let mut trailing = good.to_vec();
    trailing.extend_from_slice(b"\n{}");
    assert!(HelperReply::parse(&trailing, "probe", 1).is_err());
    assert!(HelperReply::parse(&vec![b' '; 1025], "probe", 1).is_err());
    let boot =
        br#"{"version":1,"verb":"bootstrap","result":"bootstrapped","display":1,"port":106001}"#;
    assert_eq!(
        HelperReply::parse(boot, "bootstrap", 1).unwrap(),
        HelperReply::Bootstrapped
    );
    assert!(HelperReply::parse(boot, "probe", 1).is_err());
}
#[test]
fn helper_argv_preserves_literal_unicode_metacharacters_and_fixed_binary() {
    let mut config = Config::defaults_for(std::path::Path::new(r"C:\test\manager.exe")).unwrap();
    config.launcher.program = "emacs client λ".into();
    config.launcher.args = vec!["a; $(unsafe) \"quoted\"".into()];
    let client = HelperClient::new(config).unwrap();
    let spec = client.spec("launch", Duration::from_secs(1)).unwrap();
    let args: Vec<_> = spec.args.iter().map(|a| a.to_str().unwrap()).collect();
    assert_eq!(
        args,
        vec![
            "--distribution",
            "NixOS",
            "--user",
            "Empty",
            "--exec",
            "/run/current-system/sw/bin/vcxsrv-wsl",
            "launch",
            "--",
            "emacs client λ",
            "a; $(unsafe) \"quoted\""
        ]
    );
    assert!(spec.program.is_absolute());
    assert_eq!(spec.program.file_name().unwrap(), "wsl.exe");
    assert!(matches!(
        client.spec("shell", Duration::from_secs(1)),
        Err(HelperError::Protocol)
    ));
}
#[test]
fn cancellation_ends_short_helper_without_waiting_for_deadline() {
    let cancel = Arc::new(AtomicBool::new(false));
    let other = cancel.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        other.store(true, Ordering::Release);
    });
    let start = std::time::Instant::now();
    let spec = CommandSpec {
        program: env!("CARGO_BIN_EXE_process-fixture").into(),
        args: vec!["sleep".into()],
        timeout: Duration::from_secs(10),
    };
    assert!(matches!(
        ProcessRunner::default().run_cancellable(&spec, None, &cancel),
        Err(ProcessError::Cancelled)
    ));
    assert!(start.elapsed() < Duration::from_secs(2));
    thread.join().unwrap();
}
#[test]
fn cancelled_before_start_prevents_executable_setup() {
    let flag = AtomicBool::new(true);
    let spec = CommandSpec {
        program: std::env::temp_dir().join("task6-missing-executable.exe"),
        args: vec![],
        timeout: Duration::from_secs(10),
    };
    assert!(matches!(
        ProcessRunner::default().run_cancellable(&spec, None, &flag),
        Err(ProcessError::Cancelled)
    ));
}
