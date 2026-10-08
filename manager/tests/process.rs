use std::{
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};
use vcxsrv_wsl_manager::process::{CommandSpec, ProcessError, ProcessRunner};
fn spec(mode: &str) -> CommandSpec {
    CommandSpec {
        program: PathBuf::from(env!("CARGO_BIN_EXE_process-fixture")),
        args: vec![mode.into()],
        timeout: Duration::from_secs(5),
    }
}
#[test]
fn exact_native_argv() {
    let args = [
        "--list",
        "--running",
        "--quiet",
        "",
        "NixOS 空 🦀",
        "quote\"x",
        r"end slash\",
        r"space slash \",
        r#"slash\\\"quote"#,
        "& | ; $(x) %PATH% ^ > <",
        "tab\there",
    ];
    let mut s = spec("argv");
    s.args.extend(args.iter().map(OsString::from));
    let o = ProcessRunner::default().run(&s, None).unwrap();
    let mut bytes = o.stdout.as_slice();
    let mut received = Vec::new();
    while !bytes.is_empty() {
        let n = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
        received.push(std::str::from_utf8(&bytes[4..4 + n]).unwrap());
        bytes = &bytes[4 + n..];
    }
    assert_eq!(received, args);
}
#[test]
fn stdin_complete_then_eof() {
    let o = ProcessRunner::default()
        .run(&spec("stdin"), Some(&vec![b'x'; 300_000]))
        .unwrap();
    assert_eq!(o.stdout, b"300000");
}
#[test]
fn timeout_covers_stdin_and_descendant_output() {
    for mode in ["sleep", "no-stdin", "descendant"] {
        let input = vec![b'x'; 300_000];
        let now = Instant::now();
        let mut s = spec(mode);
        s.timeout = Duration::from_millis(800);
        let e = ProcessRunner::default()
            .run(
                &s,
                if mode == "descendant" {
                    None
                } else {
                    Some(&input)
                },
            )
            .unwrap_err();
        assert_eq!(e, ProcessError::Timeout);
        assert!(
            now.elapsed() < Duration::from_secs(3),
            "{mode} elapsed {:?}",
            now.elapsed()
        );
    }
}
#[test]
fn combined_output_limit_and_safe_nonzero_error() {
    assert_eq!(
        ProcessRunner::new(8192)
            .run(&spec("flood"), None)
            .unwrap_err(),
        ProcessError::OutputLimit
    );
    assert_eq!(
        ProcessRunner::default()
            .run(&spec("exit"), None)
            .unwrap_err(),
        ProcessError::Nonzero(23)
    );
    let e = ProcessRunner::default()
        .run(&spec("secret-fail"), Some(b"PRIVATE-COOKIE-FIXTURE"))
        .unwrap_err();
    assert!(!format!("{e:?} {e}").contains("PRIVATE"));
    let mut s = spec("argv");
    s.args.push("PRIVATE-COOKIE-FIXTURE".into());
    assert!(!format!("{s:?}").contains("PRIVATE"));
}
#[test]
fn rejects_nul_and_relative_executable() {
    let mut s = spec("argv");
    s.args.push("x\0y".into());
    assert_eq!(
        ProcessRunner::default().run(&s, None).unwrap_err(),
        ProcessError::InvalidCommand
    );
    s.program = "fixture.exe".into();
    assert_eq!(
        ProcessRunner::default().run(&s, None).unwrap_err(),
        ProcessError::InvalidCommand
    );
}

#[test]
fn secret_stdin_pipe_is_restricted_to_current_user() {
    assert_eq!(
        ProcessRunner::default()
            .run(&spec("pipe-private"), None)
            .unwrap()
            .stdout,
        b"private"
    );
}

#[test]
fn unrelated_inheritable_handle_is_excluded() {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{Foundation::*, Security::SECURITY_ATTRIBUTES, System::Threading::*};
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let handle = unsafe { CreateEventW(&sa, 1, 0, null()) };
    assert!(!handle.is_null());
    let mut s = spec("inheritance");
    s.args.push((handle as usize).to_string().into());
    let result = ProcessRunner::default().run(&s, None);
    unsafe {
        CloseHandle(handle);
    }
    assert_eq!(result.unwrap().stdout, b"excluded");
}
#[test]
fn timed_out_descendant_is_terminated_by_isolated_job() {
    use windows_sys::Win32::{Foundation::*, System::Threading::*};
    let path =
        std::env::temp_dir().join(format!("vcxsrv-command-child-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut s = spec("descendant");
    s.timeout = Duration::from_secs(2);
    s.args.push(path.as_os_str().to_owned());
    let worker = std::thread::spawn(move || ProcessRunner::default().run(&s, None));
    let start = Instant::now();
    let process = loop {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(pid) = text.parse::<u32>() {
                let h = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
                assert!(!h.is_null());
                break h;
            }
        }
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "fixture did not publish descendant"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let error = worker.join().unwrap().unwrap_err();
    let ended = unsafe { WaitForSingleObject(process, 3000) };
    unsafe {
        CloseHandle(process);
    }
    let _ = std::fs::remove_file(path);
    assert_eq!(error, ProcessError::Timeout);
    assert_eq!(ended, WAIT_OBJECT_0);
}
