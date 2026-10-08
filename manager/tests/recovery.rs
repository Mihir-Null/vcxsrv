use std::{
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::PathBuf,
    process::{Child, Command},
    sync::Arc,
    time::{Duration, Instant},
};
use vcxsrv_wsl_manager::{
    authority::AuthorityStore,
    config::Config,
    instance::InstanceGuard,
    owner::ServerIdentity,
    recovery::{retire_stopped_record, RecoveryResult},
    storage::ProtectedDirectory,
};
use windows_sys::Win32::{Foundation::FILETIME, System::Threading::GetProcessTimes};
struct Fixture(Child);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn setup(name: &str) -> (PathBuf, ProtectedDirectory, Config, Fixture, ServerIdentity) {
    let scope = format!("task6-recovery-{}-{name}", std::process::id());
    let path = std::env::temp_dir().join(&scope);
    let guard = Arc::new(InstanceGuard::acquire(&scope).unwrap());
    let store = ProtectedDirectory::open_or_create(&path, guard).unwrap();
    let config =
        Config::defaults_for(std::path::Path::new(r"C:\removed-installation\manager.exe")).unwrap();
    let authority = AuthorityStore::new(&store).load_or_create(1).unwrap();
    drop(authority);
    let child = Fixture(
        Command::new(env!("CARGO_BIN_EXE_process-fixture"))
            .arg("sleep")
            .creation_flags(0x08000000)
            .spawn()
            .unwrap(),
    );
    let mut created: FILETIME = unsafe { std::mem::zeroed() };
    let mut exit = created;
    let mut kernel = created;
    let mut user = created;
    assert_ne!(
        unsafe {
            GetProcessTimes(
                child.0.as_raw_handle(),
                &mut created,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        },
        0
    );
    let identity = ServerIdentity {
        version: 1,
        pid: child.0.id(),
        creation_time: (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime),
        sid: store.sid().into(),
        image: r"C:\historical-missing-install\old.exe".into(),
        image_identity: store.identity(),
        installation: store.identity(),
        storage: store.identity(),
        distro: config.distro.clone(),
        user: config.user.clone(),
        display: config.display,
        authority: "authority".into(),
        authority_identity: store.read("authority", 64).unwrap().identity(),
        external_job: true,
    };
    drop(
        store
            .create_new("ownership.json", &serde_json::to_vec(&identity).unwrap())
            .unwrap(),
    );
    (path, store, config, child, identity)
}
#[test]
fn live_record_is_refused_and_exited_original_record_retires_without_authority_change() {
    let (path, store, config, mut child, _) = setup("signaled");
    let bytes = store.read("authority", 64).unwrap().bytes().to_vec();
    let authid = store.read("authority", 64).unwrap().identity();
    let original = store
        .read("ownership.json", 16384)
        .unwrap()
        .bytes()
        .to_vec();
    assert!(
        retire_stopped_record(&store, &config, Instant::now() + Duration::from_secs(5)).is_err()
    );
    assert_eq!(
        store.read("ownership.json", 16384).unwrap().bytes(),
        original
    );
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    assert_eq!(
        retire_stopped_record(&store, &config, Instant::now() + Duration::from_secs(5)).unwrap(),
        RecoveryResult::Retired
    );
    assert_eq!(store.read("authority", 64).unwrap().bytes(), bytes);
    assert_eq!(store.read("authority", 64).unwrap().identity(), authid);
    assert_eq!(
        retire_stopped_record(&store, &config, Instant::now() + Duration::from_secs(5)).unwrap(),
        RecoveryResult::NoRecord
    );
    drop(child);
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn absent_pid_after_closed_child_retires_only_valid_scope_and_never_recreates_authority() {
    let (path, store, config, mut child, identity) = setup("absent");
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    drop(child);
    for mutate in 0..6 {
        let mut bad = identity.clone();
        match mutate {
            0 => bad.version = 2,
            1 => bad.pid = 0,
            2 => bad.sid = "S-1-5-18".into(),
            3 => bad.storage.index += 1,
            4 => bad.distro = "Other".into(),
            _ => bad.authority_identity.index += 1,
        };
        store
            .replace("ownership.json", &serde_json::to_vec(&bad).unwrap())
            .unwrap();
        assert!(
            retire_stopped_record(&store, &config, Instant::now() + Duration::from_secs(5))
                .is_err()
        );
        assert!(path.join("ownership.json").exists());
    }
    store
        .replace("ownership.json", &serde_json::to_vec(&identity).unwrap())
        .unwrap();
    assert_eq!(
        retire_stopped_record(&store, &config, Instant::now() + Duration::from_secs(5)).unwrap(),
        RecoveryResult::Retired
    );
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
#[test]
fn synthetic_creation_mismatch_preserves_live_unrelated_child_and_unchanged_removal_refuses_rewrite(
) {
    let (path, store, config, mut child, mut identity) = setup("mismatch");
    identity.creation_time += 1;
    store
        .replace("ownership.json", &serde_json::to_vec(&identity).unwrap())
        .unwrap();
    assert_eq!(
        retire_stopped_record(&store, &config, Instant::now() + Duration::from_secs(5)).unwrap(),
        RecoveryResult::Retired
    );
    assert!(child.0.try_wait().unwrap().is_none());
    drop(store.create_new("record", b"old").unwrap());
    let old = store.read("record", 8).unwrap().identity();
    std::fs::write(path.join("record"), b"new").unwrap();
    assert!(store.remove_if_unchanged("record", old, b"old").is_err());
    assert_eq!(store.read("record", 8).unwrap().bytes(), b"new");
    store.replace("record", b"new file").unwrap();
    assert!(store
        .remove_if_unchanged("record", old, b"new file")
        .is_err());
    drop(child);
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn stopped_inspection_is_read_only_and_live_refusal_preserves_every_identity() {
    let (path, store, config, mut child, _) = setup("inspection");
    let record = store.read("ownership.json", 16384).unwrap();
    let original = record.bytes().to_vec();
    let id = record.identity();
    drop(record);
    let auth = store.read("authority", 64).unwrap();
    let authbytes = auth.bytes().to_vec();
    let authid = auth.identity();
    drop(auth);
    assert!(vcxsrv_wsl_manager::recovery::inspect_stopped_record(
        &store,
        &config,
        Instant::now() + Duration::from_secs(5)
    )
    .is_err());
    child.0.kill().unwrap();
    child.0.wait().unwrap();
    let proof = vcxsrv_wsl_manager::recovery::inspect_stopped_record(
        &store,
        &config,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    assert!(proof.is_some());
    drop(proof);
    assert_eq!(
        store.read("ownership.json", 16384).unwrap().bytes(),
        original
    );
    assert_eq!(store.read("ownership.json", 16384).unwrap().identity(), id);
    assert_eq!(store.read("authority", 64).unwrap().bytes(), authbytes);
    assert_eq!(store.read("authority", 64).unwrap().identity(), authid);
    drop(child);
    drop(store);
    std::fs::remove_dir_all(path).unwrap();
}
