use std::{
    ffi::OsString,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use vcxsrv_wsl_manager::{
    instance::InstanceGuard,
    owner::{ServerBinding, ServerOwner},
    storage::{ProtectedDirectory, StorageError},
};
fn scope(s: &str) -> String {
    format!("task5-{}-{s}", std::process::id())
}
fn directory(s: &str) -> (PathBuf, ProtectedDirectory) {
    let p = std::env::temp_dir().join(scope(s));
    let guard = Arc::new(InstanceGuard::acquire(&scope(s)).unwrap());
    let d = ProtectedDirectory::open_or_create(&p, guard).unwrap();
    (p, d)
}
#[test]
#[ignore = "requires desktop launch outside restrictive harness job; run explicitly"]
fn persistent_server_loads_module_relative_mixed_paths_and_installation_cwd() {
    let (p, d) = directory("bundle-resources");
    drop(d.create_new("authority", b"fixture-secret").unwrap());
    let installation = p.join("portable bundle");
    std::fs::create_dir_all(installation.join("xkbdata/rules")).unwrap();
    std::fs::write(installation.join("xkbdata/rules/xorg"), b"fixture-rules").unwrap();
    std::fs::write(installation.join("protocol.txt"), b"fixture-protocol").unwrap();
    let image = installation.join("server-fixture.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_process-fixture"), &image).unwrap();
    let canonical_image = std::fs::canonicalize(&image).unwrap();
    assert_ne!(std::env::current_dir().unwrap(), installation);
    let binding = ServerBinding::new(
        &d,
        image.clone(),
        "NixOS".into(),
        "Empty".into(),
        1,
        "authority",
    )
    .unwrap();
    let report = p.join("resources.json");
    let owner = ServerOwner::spawn(
        &d,
        &binding,
        &["bundle-resources".into(), report.clone().into_os_string()],
    )
    .unwrap();
    let identity = owner.identity().clone();
    let start = Instant::now();
    let result = loop {
        if let Ok(bytes) = std::fs::read(&report) {
            if let Ok(result) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                break Some(result);
            }
        }
        if start.elapsed() >= Duration::from_secs(5) {
            break None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // Exercise unchanged canonical recovery, then clean only the retained fixture
    // process before any assertions about the intentionally failing legacy reads.
    drop(owner);
    let mut owner = ServerOwner::recover(&d, &binding).unwrap().unwrap();
    assert_eq!(owner.identity(), &identity);
    owner
        .stop_confirmed(&d, Instant::now() + Duration::from_secs(5))
        .unwrap();
    drop(owner);
    drop(binding);
    drop(d);
    std::fs::remove_dir_all(p).unwrap();
    let result = result.expect("fixture did not publish its resource report");
    println!(
        "resource fixture: {result}; retained identity: {}",
        identity.image.display()
    );
    assert_eq!(identity.image, canonical_image);
    assert_eq!(result["module"], image.to_string_lossy().as_ref());
    assert_eq!(result["argv0"], image.to_string_lossy().as_ref());
    assert_eq!(result["cwd"], installation.to_string_lossy().as_ref());
    assert_eq!(result["mixed_resource"], true);
    assert_eq!(result["relative_resource"], true);
}
#[test]
fn instance_guard_excludes_duplicates_and_releases_on_drop() {
    let s = scope("mutex");
    let a = InstanceGuard::acquire(&s).unwrap();
    assert!(InstanceGuard::acquire(&s).is_err());
    let probe = vcxsrv_wsl_manager::process::CommandSpec {
        program: env!("CARGO_BIN_EXE_process-fixture").into(),
        args: vec!["instance".into(), s.clone().into()],
        timeout: Duration::from_secs(5),
    };
    assert_eq!(
        vcxsrv_wsl_manager::process::ProcessRunner::default()
            .run(&probe, None)
            .unwrap()
            .stdout,
        b"denied"
    );
    let b = InstanceGuard::acquire(&scope("other")).unwrap();
    drop(a);
    assert_eq!(
        vcxsrv_wsl_manager::process::ProcessRunner::default()
            .run(&probe, None)
            .unwrap()
            .stdout,
        b"acquired"
    );
    assert!(InstanceGuard::acquire(&s).is_ok());
    drop(b);
}
#[test]
fn private_storage_roundtrip_atomic_replace_rejects_ordinary_file_and_hardlink() {
    let (p, d) = directory("storage");
    let first = d.create_new("authority", b"old").unwrap();
    let id = first.identity();
    drop(first);
    assert_eq!(d.read("authority", 32).unwrap().bytes(), b"old");
    d.replace("authority", b"new").unwrap();
    let f = d.read("authority", 32).unwrap();
    assert_eq!(f.bytes(), b"new");
    assert_ne!(f.identity(), id);
    drop(f);
    assert!(d.read("authority", 2).is_err());
    std::fs::write(p.join("ordinary"), b"unsafe").unwrap();
    assert!(matches!(d.read("ordinary", 32), Err(StorageError::Unsafe)));
    std::fs::hard_link(p.join("authority"), p.join("hardlink")).unwrap();
    assert!(d.read("authority", 32).is_err());
    std::fs::remove_file(p.join("hardlink")).unwrap();
    assert!(d.create_new("../escape", b"no").is_err());
    drop(d);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn held_ancestor_cannot_be_renamed_or_replaced() {
    let (p, d) = directory("ancestors");
    assert!(std::fs::rename(&p, p.with_extension("renamed")).is_err());
    drop(d);
    std::fs::rename(&p, p.with_extension("renamed")).unwrap();
    std::fs::remove_dir(p.with_extension("renamed")).unwrap();
}
#[test]
fn ordinary_directory_is_rejected_without_acl_repair() {
    let p = std::env::temp_dir().join(scope("ordinary-dir"));
    std::fs::create_dir(&p).unwrap();
    let g = Arc::new(InstanceGuard::acquire(&scope("ordinary-dir")).unwrap());
    assert!(ProtectedDirectory::open_or_create(&p, g).is_err());
    std::fs::remove_dir(p).unwrap();
}
#[test]
#[ignore = "requires desktop launch outside restrictive harness job; run explicitly"]
fn persistent_handle_detaches_recovers_and_stops_only_valid_identity() {
    let (p, d) = directory("owner");
    drop(d.create_new("authority", b"fixture-secret").unwrap());
    std::fs::copy(
        env!("CARGO_BIN_EXE_process-fixture"),
        p.join("server-fixture.exe"),
    )
    .unwrap();
    let b = ServerBinding::new(
        &d,
        p.join("server-fixture.exe"),
        "NixOS".into(),
        "Empty".into(),
        1,
        "authority",
    )
    .unwrap();
    let owner = ServerOwner::spawn(&d, &b, &[OsString::from("sleep")]).unwrap();
    let record = owner.identity().clone();
    assert!(owner.is_alive().unwrap());
    drop(owner);
    let mut owner = ServerOwner::recover(&d, &b).unwrap().unwrap();
    assert_eq!(owner.identity(), &record);
    assert!(owner.is_alive().unwrap());
    // A disposable helper job closing must not destroy the independent server.
    let e = vcxsrv_wsl_manager::process::ProcessRunner::default().run(
        &vcxsrv_wsl_manager::process::CommandSpec {
            program: PathBuf::from(env!("CARGO_BIN_EXE_process-fixture")),
            args: vec!["sleep".into()],
            timeout: Duration::from_millis(100),
        },
        None,
    );
    assert!(e.is_err());
    assert!(owner.is_alive().unwrap());
    assert!(ServerOwner::spawn(&d, &b, &["sleep".into()]).is_err());
    let mut mutations = Vec::new();
    let mut bad = record.clone();
    bad.creation_time += 1;
    mutations.push(bad);
    let mut bad = record.clone();
    bad.sid = "S-1-5-18".into();
    mutations.push(bad);
    let mut bad = record.clone();
    bad.image_identity.index += 1;
    mutations.push(bad);
    let mut bad = record.clone();
    bad.installation.index += 1;
    mutations.push(bad);
    let mut bad = record.clone();
    bad.authority_identity.index += 1;
    mutations.push(bad);
    let mut bad = record.clone();
    bad.distro = "OtherGuest".into();
    mutations.push(bad);
    let mut bad = record.clone();
    bad.pid = std::process::id();
    mutations.push(bad);
    for bad in mutations {
        d.replace("ownership.json", &serde_json::to_vec(&bad).unwrap())
            .unwrap();
        assert!(ServerOwner::recover(&d, &b).is_err());
        assert!(owner
            .stop_confirmed(&d, Instant::now() + Duration::from_secs(1))
            .is_err());
        assert!(owner.is_alive().unwrap());
    }
    d.replace("ownership.json", &serde_json::to_vec(&record).unwrap())
        .unwrap();
    owner
        .stop_confirmed(&d, Instant::now() + Duration::from_secs(5))
        .unwrap();
    assert!(!owner.is_alive().unwrap());
    assert!(ServerOwner::recover(&d, &b).unwrap().is_none());
    drop(owner);
    drop(b);
    let authority = d.read("authority", 64).unwrap().identity();
    d.remove("authority", authority).unwrap();
    assert!(ServerBinding::new(
        &d,
        p.join("server-fixture.exe"),
        "NixOS".into(),
        "Empty".into(),
        1,
        "authority"
    )
    .is_err());
    drop(d);
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
#[ignore = "manual native job diagnostic; does not establish persistence"]
fn inspect_fixture_parent_job() {
    use windows_sys::Win32::{
        Foundation::*,
        System::{JobObjects::*, Threading::*},
    };
    unsafe {
        let mut present = 0;
        assert_ne!(
            IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut present),
            0
        );
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        let ok = QueryInformationJobObject(
            std::ptr::null_mut(),
            JobObjectExtendedLimitInformation,
            (&mut info as *mut JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&info) as u32,
            std::ptr::null_mut(),
        );
        println!(
            "job present={present}, query={ok}, error={}, flags={:x}",
            GetLastError(),
            info.BasicLimitInformation.LimitFlags
        );
    }
}
#[test]
fn read_only_inspection_coexists_with_manager_and_cannot_write() {
    let (p, d) = directory("inspect");
    drop(d.create_new("sample", b"safe").unwrap());
    let reader = ProtectedDirectory::inspect(&p).unwrap();
    assert_eq!(reader.read("sample", 16).unwrap().bytes(), b"safe");
    assert!(reader.replace("sample", b"bad").is_err());
    assert!(reader.create_new("other", b"bad").is_err());
    drop(reader);
    drop(d);
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn failed_publication_keeps_original_and_leaves_no_temp_file() {
    let (p, d) = directory("publication");
    let held = d.create_new("authority", b"original").unwrap();
    assert!(d.replace("authority", b"replacement").is_err());
    assert!(d.create_new("authority", b"replacement").is_err());
    assert!(d
        .create_new("too-large", &vec![0; 1024 * 1024 + 1])
        .is_err());
    assert_eq!(held.bytes(), b"original");
    assert_eq!(std::fs::read_dir(&p).unwrap().count(), 1);
    drop(held);
    assert_eq!(d.read("authority", 64).unwrap().bytes(), b"original");
    drop(d);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn unsafe_null_and_world_dacls_are_rejected_without_repair() {
    use std::{
        os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle},
        ptr::null_mut,
    };
    use windows_sys::Win32::{
        Foundation::*,
        Security::{Authorization::*, *},
    };
    let (p, d) = directory("acls");
    for (name, sddl) in [("null", None), ("world", Some("D:P(A;;GA;;;WD)"))] {
        drop(d.create_new(name, b"unchanged").unwrap());
        let f = std::fs::OpenOptions::new()
            .access_mode(GENERIC_READ | GENERIC_WRITE | 0x00040000)
            .open(p.join(name))
            .unwrap();
        unsafe {
            let mut sd = null_mut();
            let mut acl = null_mut();
            if let Some(sddl) = sddl {
                let text: Vec<u16> = std::ffi::OsStr::new(sddl)
                    .encode_wide()
                    .chain(Some(0))
                    .collect();
                assert_ne!(
                    ConvertStringSecurityDescriptorToSecurityDescriptorW(
                        text.as_ptr(),
                        SDDL_REVISION_1,
                        &mut sd,
                        null_mut()
                    ),
                    0
                );
                let mut present = 0;
                let mut defaulted = 0;
                assert_ne!(
                    GetSecurityDescriptorDacl(sd, &mut present, &mut acl, &mut defaulted),
                    0
                );
            }
            assert_eq!(
                SetSecurityInfo(
                    f.as_raw_handle(),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    null_mut(),
                    null_mut(),
                    acl,
                    null_mut()
                ),
                0
            );
            if !sd.is_null() {
                LocalFree(sd);
            }
        }
        drop(f);
        assert!(matches!(d.read(name, 64), Err(StorageError::Unsafe)));
        assert!(d.replace(name, b"bad").is_err());
        assert_eq!(std::fs::read(p.join(name)).unwrap(), b"unchanged");
    }
    drop(d);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn precreated_unsafe_mutex_and_wrong_object_type_fail_closed() {
    use std::{os::windows::ffi::OsStrExt, ptr::null};
    use windows_sys::Win32::{Foundation::*, System::Threading::*};
    let identity = InstanceGuard::acquire(&scope("sid")).unwrap();
    let sid = identity.sid().to_owned();
    drop(identity);
    for kind in ["unsafe-mutex", "event"] {
        let s = scope(kind);
        let name: Vec<u16> = std::ffi::OsStr::new(&format!(r"Global\VcXsrvWSL.{s}.{sid}"))
            .encode_wide()
            .chain(Some(0))
            .collect();
        let h = unsafe {
            if kind == "event" {
                CreateEventW(null(), 0, 0, name.as_ptr())
            } else {
                CreateMutexW(null(), 0, name.as_ptr())
            }
        };
        assert!(!h.is_null());
        assert!(InstanceGuard::acquire(&s).is_err());
        unsafe {
            CloseHandle(h);
        }
    }
}
#[test]
fn junction_ancestor_is_rejected() {
    use std::{os::windows::process::CommandExt, process::Command};
    let (p, d) = directory("junction");
    let target = p.join("target");
    std::fs::create_dir(&target).unwrap();
    let link = p.join("link");
    let command = format!(
        "New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null",
        link.display().to_string().replace('\'', "''"),
        target.display().to_string().replace('\'', "''")
    );
    let output = Command::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(output.status.success(), "junction fixture creation failed");
    assert!(ProtectedDirectory::inspect(&link).is_err());
    assert!(ProtectedDirectory::inspect(&link.join("child")).is_err());
    // remove_dir removes the junction, never recursively follows its target.
    std::fs::remove_dir(link).unwrap();
    drop(d);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
#[ignore = "manual native job diagnostic; does not establish persistence"]
fn persistent_job_report() {
    let (p, d) = directory("job-report");
    drop(d.create_new("authority", b"fixture").unwrap());
    std::fs::copy(env!("CARGO_BIN_EXE_process-fixture"), p.join("server.exe")).unwrap();
    let b = ServerBinding::new(
        &d,
        p.join("server.exe"),
        "NixOS".into(),
        "Empty".into(),
        1,
        "authority",
    )
    .unwrap();
    let result = ServerOwner::spawn(
        &d,
        &b,
        &["job-report".into(), p.join("job.txt").into_os_string()],
    );
    println!("spawn error {:?}", result.as_ref().err());
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(3) {
        if let Ok(info) = std::fs::read_to_string(p.join("job.txt")) {
            println!("child {info}");
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_secs(3));
    drop(result);
    drop(b);
    drop(d);
    std::fs::remove_dir_all(p).unwrap();
}
#[test]
fn suspended_start_failure_does_not_run_fixture_code_or_publish_ownership() {
    let spec = vcxsrv_wsl_manager::process::CommandSpec {
        program: env!("CARGO_BIN_EXE_process-fixture").into(),
        args: vec!["owner-start-failure".into()],
        timeout: Duration::from_secs(5),
    };
    assert_eq!(
        vcxsrv_wsl_manager::process::ProcessRunner::default()
            .run(&spec, None)
            .unwrap()
            .stdout,
        b"safe-failure"
    );
}
#[test]
#[ignore = "requires desktop launch; explicitly verifies real manager process exit"]
fn manager_normal_and_abrupt_exit_preserve_recoverable_fixture() {
    use std::{os::windows::process::CommandExt, process::Command};
    for mode in ["normal", "abrupt"] {
        let path = std::env::temp_dir().join(scope(mode));
        let launcher = path.with_extension("exe");
        std::fs::copy(env!("CARGO_BIN_EXE_process-fixture"), &launcher).unwrap();
        let status = Command::new(&launcher)
            .args([
                OsString::from("owner-exit"),
                path.as_os_str().to_owned(),
                scope(mode).into(),
                mode.into(),
            ])
            .creation_flags(0x08000000)
            .status()
            .unwrap();
        assert!(status.success());
        let guard = Arc::new(InstanceGuard::acquire(&scope(mode)).unwrap());
        let store = ProtectedDirectory::open_or_create(&path, guard).unwrap();
        let binding = ServerBinding::new(
            &store,
            launcher.clone(),
            "NixOS".into(),
            "Empty".into(),
            1,
            "authority",
        )
        .unwrap();
        let mut owner = ServerOwner::recover(&store, &binding).unwrap().unwrap();
        assert!(owner.is_alive().unwrap());
        owner
            .stop_confirmed(&store, Instant::now() + Duration::from_secs(5))
            .unwrap();
        drop(owner);
        drop(binding);
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
        std::fs::remove_file(launcher).unwrap();
    }
}
