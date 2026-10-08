use std::{
    os::windows::process::CommandExt,
    process::Command,
    ptr::null,
    time::{Duration, Instant},
};
static TRAY_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
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
    let _serial = TRAY_SERIAL.lock().unwrap();
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
    // Real modal message pumps: nested genuine worker notifications and Explorer restore.
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, CMD_STOP as usize, 0);
    }
    let dialog = wait_owned_window(child.id(), "#32770", Some("Confirm Stop server"));
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, CMD_START as usize, 0);
        PostMessageW(hwnd, WM_APP + 2, 0, 0);
    }
    assert_ne!(
        unsafe { SendMessageTimeoutW(hwnd, message, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut response) },
        0
    );
    assert_eq!(response, 1);
    assert_ne!(
        unsafe { SendMessageTimeoutW(hwnd, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut response) },
        0
    );
    unsafe {
        PostMessageW(dialog, WM_COMMAND, IDNO as usize, 0);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while unsafe { IsWindow(dialog) } != 0 {
        assert!(Instant::now() < deadline, "Stop modal did not close");
        std::thread::sleep(Duration::from_millis(20));
    }
    unsafe {
        PostMessageW(hwnd, WM_APP + 1, 0, WM_CONTEXTMENU as isize);
    }
    let _menu = wait_owned_window(child.id(), "#32768", None);
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, CMD_START as usize, 0);
        PostMessageW(hwnd, WM_APP + 2, 0, 0);
    }
    assert_ne!(
        unsafe { SendMessageTimeoutW(hwnd, message, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut response) },
        0
    );
    assert_eq!(response, 1);
    unsafe {
        PostMessageW(hwnd, WM_CANCELMODE, 0, 0);
    }
    assert_ne!(
        unsafe { SendMessageTimeoutW(hwnd, WM_NULL, 0, 0, SMTO_ABORTIFHUNG, 1000, &mut response) },
        0
    );
    // A subsequent action and Exit inside another modal must still dispatch.
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, CMD_START as usize, 0);
    }
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, CMD_STOP as usize, 0);
    }
    let _exit_dialog = wait_owned_window(child.id(), "#32770", Some("Confirm Stop server"));
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
#[test]
fn actual_tray_rejects_missing_explicit_config_but_creates_implicit_default() {
    let _serial = TRAY_SERIAL.lock().unwrap();
    let path = std::env::temp_dir().join(format!("task6-explicit-config-{}", std::process::id()));
    let guard = std::sync::Arc::new(
        vcxsrv_wsl_manager::instance::InstanceGuard::acquire(&format!(
            "task6-explicit-config-{}",
            std::process::id()
        ))
        .unwrap(),
    );
    let directory =
        vcxsrv_wsl_manager::storage::ProtectedDirectory::open_or_create(&path, guard).unwrap();
    drop(directory);
    let missing = path.join("mistyped.json");
    let mut child = Fixture(
        Command::new(env!("CARGO_BIN_EXE_vcxsrv-wsl-manager"))
            .args(["--tray", "--state-dir"])
            .arg(&path)
            .arg("--config")
            .arg(&missing)
            .creation_flags(0x08000000)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            panic!("explicit missing config did not fail tray startup");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    assert!(!status.success());
    assert!(!missing.exists());
    assert!(!path.join("authority").exists());
    assert!(!path.join("ownership.json").exists());
    assert!(!path.join("config.json").exists());
    let mut child = Fixture(
        Command::new(env!("CARGO_BIN_EXE_vcxsrv-wsl-manager"))
            .args(["--tray", "--state-dir"])
            .arg(&path)
            .creation_flags(0x08000000)
            .spawn()
            .unwrap(),
    );
    let class: Vec<u16> = "VcXsrvWSL.Manager".encode_utf16().chain(Some(0)).collect();
    let deadline = Instant::now() + Duration::from_secs(5);
    let hwnd = loop {
        let hwnd = unsafe { FindWindowW(class.as_ptr(), null()) };
        if !hwnd.is_null() && path.join("config.json").exists() {
            break hwnd;
        }
        assert!(child.try_wait().unwrap().is_none());
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    };
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, CMD_STOP as usize, 0);
    }
    let _exit_dialog = wait_owned_window(child.id(), "#32770", Some("Confirm Stop server"));
    unsafe {
        PostMessageW(hwnd, WM_COMMAND, CMD_EXIT as usize, 0);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!path.join("authority").exists());
    assert!(!path.join("ownership.json").exists());
    std::fs::remove_dir_all(path).unwrap();
}
fn wait_owned_window(
    pid: u32,
    class: &str,
    title: Option<&str>,
) -> windows_sys::Win32::Foundation::HWND {
    struct Search<'a> {
        pid: u32,
        class: &'a str,
        title: Option<&'a str>,
        found: windows_sys::Win32::Foundation::HWND,
    }
    unsafe extern "system" fn visit(
        hwnd: windows_sys::Win32::Foundation::HWND,
        parameter: isize,
    ) -> i32 {
        let search = &mut *(parameter as *mut Search<'_>);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != search.pid {
            return 1;
        }
        let mut buffer = [0u16; 256];
        let n = GetClassNameW(hwnd, buffer.as_mut_ptr(), 256);
        if String::from_utf16_lossy(&buffer[..n as usize]) != search.class {
            return 1;
        }
        if let Some(title) = search.title {
            let n = GetWindowTextW(hwnd, buffer.as_mut_ptr(), 256);
            if String::from_utf16_lossy(&buffer[..n as usize]) != title {
                return 1;
            }
        }
        search.found = hwnd;
        0
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let mut search = Search {
            pid,
            class,
            title,
            found: std::ptr::null_mut(),
        };
        unsafe {
            EnumWindows(Some(visit), (&mut search as *mut Search<'_>) as isize);
        }
        if !search.found.is_null() {
            return search.found;
        }
        assert!(
            Instant::now() < deadline,
            "native modal/menu did not appear"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
