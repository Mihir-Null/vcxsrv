use std::{
    os::windows::process::CommandExt,
    process::Command,
    ptr::null,
    time::{Duration, Instant},
};
struct Fixture(std::process::Child);
impl Drop for Fixture {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
impl std::ops::Deref for Fixture {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for Fixture {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
use vcxsrv_wsl_manager::{
    controller::Action,
    tray::{command_action, CMD_EXIT, CMD_LAUNCH, CMD_REFRESH, CMD_START, CMD_STOP},
};
use windows_sys::Win32::UI::WindowsAndMessaging::*;
#[test]
fn tray_start_launch_refresh_dispatch_typed_actions_and_stop_needs_confirmation() {
    assert_eq!(command_action(CMD_START, false), Some(Action::StartServer));
    assert_eq!(command_action(CMD_LAUNCH, false), Some(Action::Launch));
    assert_eq!(command_action(CMD_REFRESH, false), Some(Action::Refresh));
    assert_eq!(command_action(CMD_STOP, false), None);
    assert_eq!(
        command_action(CMD_STOP, true),
        Some(Action::StopServerConfirmed)
    );
    assert_eq!(command_action(CMD_EXIT, true), None);
}
#[test]
fn actual_manager_tray_missing_companion_is_responsive_restores_icon_and_exits() {
    let path = std::env::temp_dir().join(format!("task6-tray-{}", std::process::id()));
    let guard = std::sync::Arc::new(
        vcxsrv_wsl_manager::instance::InstanceGuard::acquire(&format!(
            "task6-tray-config-{}",
            std::process::id()
        ))
        .unwrap(),
    );
    let directory =
        vcxsrv_wsl_manager::storage::ProtectedDirectory::open_or_create(&path, guard).unwrap();
    let mut config = vcxsrv_wsl_manager::config::Config::defaults_for(std::path::Path::new(env!(
        "CARGO_BIN_EXE_vcxsrv-wsl-manager"
    )))
    .unwrap();
    config.server_path = path.join("missing-companion.exe");
    drop(
        directory
            .create_new("config.json", &serde_json::to_vec(&config).unwrap())
            .unwrap(),
    );
    drop(directory);
    let mut child = Fixture(
        Command::new(env!("CARGO_BIN_EXE_vcxsrv-wsl-manager"))
            .args(["--tray", "--state-dir"])
            .arg(&path)
            .creation_flags(0x08000000)
            .spawn()
            .unwrap(),
    );
    let class: Vec<u16> = "VcXsrvWSL.Manager".encode_utf16().chain(Some(0)).collect();
    let start = Instant::now();
    let hwnd = loop {
        let hwnd = unsafe { FindWindowW(class.as_ptr(), null()) };
        if !hwnd.is_null() {
            break hwnd;
        }
        assert!(
            child.try_wait().unwrap().is_none(),
            "manager exited before tray window"
        );
        assert!(
            start.elapsed() < Duration::from_secs(8),
            "tray window timeout"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut response = 0;
    assert_ne!(
        unsafe { SendMessageTimeoutW(hwnd, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut response) },
        0
    );
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, CMD_START as usize, 0);
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(text) = std::fs::read_to_string(path.join("manager.log")) {
            if text.contains("Companion vcxsrv.exe is missing") {
                break;
            }
        }
        assert!(Instant::now() < deadline, "missing companion not diagnosed");
        std::thread::sleep(Duration::from_millis(20));
    }
    let second = Command::new(env!("CARGO_BIN_EXE_vcxsrv-wsl-manager"))
        .args(["--tray", "--state-dir"])
        .arg(&path)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(
        !second.status.success(),
        "second actual manager was not rejected"
    );
    let doctor = Command::new(env!("CARGO_BIN_EXE_vcxsrv-wsl-manager"))
        .args(["doctor", "--json", "--state-dir"])
        .arg(&path)
        .creation_flags(0x08000000)
        .output()
        .unwrap();
    assert!(doctor.status.success());
    let data: serde_json::Value = serde_json::from_slice(&doctor.stdout).unwrap();
    assert_eq!(data["ready"], false);
    assert!(data["last_authenticated_state"].is_null());
    let name: Vec<u16> = "TaskbarCreated".encode_utf16().chain(Some(0)).collect();
    let message = unsafe { RegisterWindowMessageW(name.as_ptr()) };
    assert_ne!(
        unsafe { SendMessageTimeoutW(hwnd, message, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut response) },
        0
    );
    assert_eq!(response, 1, "icon was not re-added/versioned");
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, CMD_EXIT as usize, 0);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline, "tray exit blocked");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!path.join("authority").exists());
    std::fs::remove_dir_all(path).unwrap();
}
