use std::{path::PathBuf, sync::Arc, time::Duration};
use vcxsrv_wsl_manager::{config::Config, instance::InstanceGuard, storage::ProtectedDirectory};
fn state(name: &str) -> (PathBuf, ProtectedDirectory) {
    let root = std::env::temp_dir().join(format!("task8-guard-{}-{name}", std::process::id()));
    let guard = Arc::new(
        InstanceGuard::acquire(&format!("task8-setup-{}-{name}", std::process::id())).unwrap(),
    );
    let store = ProtectedDirectory::open_or_create(&root, guard).unwrap();
    (root, store)
}
#[test]
fn external_companion_without_ownership_is_refused_and_config_is_preserved() {
    let (state, store) = state("external");
    let root = state.join("destination");
    let mut config = Config::defaults_for(&root.join("vcxsrv-wsl-manager.exe")).unwrap();
    config.server_path = state.join("foreign/vcxsrv.exe");
    let bytes = serde_json::to_vec(&config).unwrap();
    drop(store.create_new("config.json", &bytes).unwrap());
    let id = store.read("config.json", 65536).unwrap().identity();
    let result = vcxsrv_wsl_manager::installation::serve(
        &root,
        &state,
        &format!("task8-check-{}-external", std::process::id()),
    );
    assert!(result.unwrap_err().contains("Configured companion"));
    assert_eq!(store.read("config.json", 65536).unwrap().bytes(), bytes);
    assert_eq!(store.read("config.json", 65536).unwrap().identity(), id);
    assert!(!state.join("ownership.json").exists());
    assert!(!state.join("authority").exists());
    drop(store);
    std::fs::remove_file(state.join("config.json")).unwrap();
    std::fs::remove_dir(state).unwrap();
}
#[test]
fn trailing_separator_cannot_hide_live_install_process() {
    let (state, store) = state("separator");
    let root = state.join("install");
    std::fs::create_dir(&root).unwrap();
    let image = root.join("vcxsrv.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_process-fixture"), &image).unwrap();
    let mut child = std::process::Command::new(&image)
        .arg("sleep")
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(30));
    let trailing = PathBuf::from(format!("{}\\", root.display()));
    let result = vcxsrv_wsl_manager::installation::serve(
        &trailing,
        &state,
        &format!("task8-check-{}-separator", std::process::id()),
    );
    let alive = child.try_wait().unwrap().is_none();
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(alive);
    assert!(result.unwrap_err().contains("running process"));
    drop(store);
    std::fs::remove_file(image).unwrap();
    std::fs::remove_dir(root).unwrap();
    std::fs::remove_dir(state).unwrap();
}
