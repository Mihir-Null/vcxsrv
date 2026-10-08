//! Hidden native top-level window: broadcasts, tray icon and UI live on this thread.
use crate::{cli::Options, controller::Action, native::wide, startup, worker::Worker};
use std::{
    ffi::OsStr,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    System::LibraryLoader::GetModuleHandleW,
    UI::{Shell::*, WindowsAndMessaging::*},
};
pub const CMD_START: u32 = 1;
pub const CMD_LAUNCH: u32 = 2;
pub const CMD_REFRESH: u32 = 3;
pub const CMD_DIAGNOSTICS: u32 = 4;
pub const CMD_CONFIG: u32 = 5;
pub const CMD_LOGS: u32 = 6;
pub const CMD_STARTUP: u32 = 7;
pub const CMD_EXIT: u32 = 8;
pub const CMD_STOP: u32 = 9;
pub const CMD_RECOVER: u32 = 10;
const KEY_SELECT: u32 = NIN_SELECT | 1;
const CALLBACK: u32 = WM_APP + 1;
const WORKER_UPDATE: u32 = WM_APP + 2;
pub fn command_action(command: u32, confirmed: bool) -> Option<Action> {
    match command {
        CMD_START => Some(Action::StartServer),
        CMD_LAUNCH => Some(Action::Launch),
        CMD_REFRESH => Some(Action::Refresh),
        CMD_RECOVER => Some(Action::RecoverStoppedRecord),
        CMD_STOP if confirmed => Some(Action::StopServerConfirmed),
        _ => None,
    }
}
struct Ui {
    worker: Option<Worker>,
    options: Options,
    taskbar_created: u32,
    icon_present: bool,
    icon_restores: u32,
}
impl Ui {
    fn status(&self) -> String {
        self.worker
            .as_ref()
            .map(|w| w.latest.lock().unwrap().status())
            .unwrap_or_else(|| "Starting manager".into())
    }
    fn icon(&mut self, hwnd: HWND, message: u32) -> bool {
        let mut icon: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        icon.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        icon.hWnd = hwnd;
        icon.uID = 1;
        icon.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
        icon.uCallbackMessage = CALLBACK;
        icon.hIcon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
        let tip = format!("VcXsrv WSL: {}", self.status());
        let units: Vec<u16> = tip.encode_utf16().take(127).collect();
        icon.szTip[..units.len()].copy_from_slice(&units);
        let result = unsafe { Shell_NotifyIconW(message, &icon) } != 0;
        if message == NIM_ADD && result {
            icon.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            self.icon_present = unsafe { Shell_NotifyIconW(NIM_SETVERSION, &icon) } != 0;
        } else if message == NIM_DELETE {
            self.icon_present = false;
        }
        result && (message != NIM_ADD || self.icon_present)
    }
    fn menu(&mut self, hwnd: HWND) {
        let menu = unsafe { CreatePopupMenu() };
        if menu.is_null() {
            return;
        }
        let busy = self.worker.as_ref().is_some_and(|w| w.queue.busy());
        fn item(menu: HMENU, id: u32, text: &str, flags: u32) {
            if let Ok(text) = wide(OsStr::new(text)) {
                unsafe {
                    AppendMenuW(menu, MF_STRING | flags, id as usize, text.as_ptr());
                }
            }
        }
        item(menu, 0, &self.status(), MF_DISABLED);
        for (id, text) in [
            (CMD_START, "Start server"),
            (CMD_LAUNCH, "Launch Emacs"),
            (CMD_REFRESH, "Refresh connection"),
        ] {
            item(menu, id, text, if busy { MF_GRAYED } else { 0 });
        }
        unsafe {
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
        }
        for (id, text) in [
            (CMD_DIAGNOSTICS, "Diagnostics"),
            (CMD_CONFIG, "Open config"),
            (CMD_LOGS, "Open logs"),
        ] {
            item(menu, id, text, 0);
        }
        let exe = std::env::current_exe().ok();
        let enabled = exe
            .as_ref()
            .is_some_and(|exe| startup::enabled(exe, &self.options).unwrap_or(false));
        item(
            menu,
            CMD_STARTUP,
            "Start manager at sign-in",
            if enabled { MF_CHECKED } else { 0 },
        );
        unsafe {
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
        }
        item(
            menu,
            CMD_RECOVER,
            "Recover stopped server record",
            if busy { MF_GRAYED } else { 0 },
        );
        item(
            menu,
            CMD_STOP,
            "Stop server…",
            if busy { MF_GRAYED } else { 0 },
        );
        item(menu, CMD_EXIT, "Exit manager (leave server running)", 0);
        let mut point: POINT = unsafe { std::mem::zeroed() };
        unsafe {
            GetCursorPos(&mut point);
            SetForegroundWindow(hwnd);
        }
        let choice = unsafe {
            TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                0,
                hwnd,
                null(),
            )
        } as u32;
        unsafe {
            DestroyMenu(menu);
            PostMessageW(hwnd, WM_NULL, 0, 0);
        }
        if choice != 0 {
            self.command(hwnd, choice);
        }
    }
    fn command(&mut self, hwnd: HWND, command: u32) {
        if command == CMD_EXIT {
            if let Some(worker) = &self.worker {
                worker.queue.shutdown();
            }
            self.icon(hwnd, NIM_DELETE);
            unsafe {
                DestroyWindow(hwnd);
            }
            return;
        }
        let confirmed = command == CMD_STOP
            && unsafe {
                MessageBoxW(hwnd,wide(OsStr::new("Stop the owned display server? Connected X11 applications will disconnect. Exit manager leaves it running.")).unwrap().as_ptr(),wide(OsStr::new("Confirm Stop server")).unwrap().as_ptr(),MB_YESNO|MB_ICONWARNING|MB_DEFBUTTON2)
            } == IDYES;
        if let Some(action) = command_action(command, confirmed) {
            if let Some(worker) = &self.worker {
                worker.queue.submit(action);
            }
            return;
        }
        match command {
            CMD_DIAGNOSTICS => {
                if let Some(worker) = &self.worker {
                    let s = worker.latest.lock().unwrap().clone();
                    let text=format!("{}\nCompanion present: {}\nValidated authority held: {}\nOwned server held: {}\nReady is the last authenticated observation; diagnostics do not poll the guest.",s.status(),s.companion_present,s.authority_valid,s.ownership_present);
                    message(hwnd, &text);
                }
            }
            CMD_CONFIG | CMD_LOGS => {
                let path = if command == CMD_CONFIG {
                    self.options.config_path()
                } else {
                    self.options.state_path().map(|p| p.join("manager.log"))
                };
                match path {
                    Ok(path) => {
                        if let Ok(path) = wide(path.as_os_str()) {
                            let result = unsafe {
                                ShellExecuteW(
                                    hwnd,
                                    wide(OsStr::new("open")).unwrap().as_ptr(),
                                    path.as_ptr(),
                                    null(),
                                    null(),
                                    SW_SHOWNORMAL,
                                )
                            };
                            if result as usize <= 32 {
                                message(hwnd,"Could not open the private file; check its path and Windows file association");
                            }
                        }
                    }
                    Err(error) => message(hwnd, error),
                }
            }
            CMD_STARTUP => match std::env::current_exe()
                .map_err(|_| "Manager path unavailable")
                .and_then(|exe| startup::toggle(&exe, &self.options))
            {
                Ok(_) => {}
                Err(error) => message(hwnd, error),
            },
            _ => {}
        }
    }
}
fn message(hwnd: HWND, text: &str) {
    let text = wide(OsStr::new(text)).unwrap();
    let title = wide(OsStr::new("VcXsrv WSL Manager")).unwrap();
    unsafe {
        MessageBoxW(
            hwnd,
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
unsafe extern "system" fn procedure(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = &*(lparam as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
    if pointer.is_null() {
        return DefWindowProcW(hwnd, message, wparam, lparam);
    }
    let ui = &mut *pointer;
    if message == ui.taskbar_created {
        ui.icon_restores += 1;
        ui.icon(hwnd, NIM_DELETE);
        return ui.icon(hwnd, NIM_ADD) as isize;
    }
    match message {
        CALLBACK
            if matches!(
                (lparam as u32) & 0xffff,
                WM_CONTEXTMENU | WM_RBUTTONUP | NIN_SELECT | KEY_SELECT
            ) =>
        {
            ui.menu(hwnd);
            0
        }
        WORKER_UPDATE => {
            ui.icon(hwnd, NIM_MODIFY);
            0
        }
        WM_COMMAND => {
            ui.command(hwnd, (wparam & 0xffff) as u32);
            0
        }
        WM_CLOSE => {
            ui.command(hwnd, CMD_EXIT);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
pub fn run(options: Options) -> Result<(), &'static str> {
    let class = wide(OsStr::new("VcXsrvWSL.Manager")).unwrap();
    let title = wide(OsStr::new("VcXsrv WSL Manager")).unwrap();
    let module = unsafe { GetModuleHandleW(null()) };
    let mut wc: WNDCLASSW = unsafe { std::mem::zeroed() };
    wc.lpfnWndProc = Some(procedure);
    wc.hInstance = module;
    wc.lpszClassName = class.as_ptr();
    if unsafe { RegisterClassW(&wc) } == 0 {
        return Err("Could not register native tray window");
    }
    let taskbar =
        unsafe { RegisterWindowMessageW(wide(OsStr::new("TaskbarCreated")).unwrap().as_ptr()) };
    let mut ui = Box::new(Ui {
        worker: None,
        options: options.clone(),
        taskbar_created: taskbar,
        icon_present: false,
        icon_restores: 0,
    });
    // A hidden top-level window receives Explorer broadcasts; HWND_MESSAGE would not.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            module,
            (&mut *ui as *mut Ui).cast(),
        )
    };
    if hwnd.is_null() {
        unsafe {
            UnregisterClassW(class.as_ptr(), module);
        }
        return Err("Could not create native tray window");
    }
    let address = hwnd as usize;
    let worker = Worker::start(options, move || unsafe {
        PostMessageW(address as HWND, WORKER_UPDATE, 0, 0);
    });
    match worker {
        Ok(worker) => ui.worker = Some(worker),
        Err(error) => {
            unsafe {
                DestroyWindow(hwnd);
                UnregisterClassW(class.as_ptr(), module);
            }
            return Err(error);
        }
    }
    if !ui.icon(hwnd, NIM_ADD) {
        if let Some(worker) = &ui.worker {
            worker.queue.shutdown();
        }
        unsafe {
            DestroyWindow(hwnd);
            UnregisterClassW(class.as_ptr(), module);
        }
        return Err("Windows notification area is unavailable");
    }
    // Passive startup may create the owned server, but cannot wake the configured guest.
    ui.worker
        .as_ref()
        .unwrap()
        .queue
        .submit(Action::StartServer);
    let mut msg: MSG = unsafe { std::mem::zeroed() };
    loop {
        let result = unsafe { GetMessageW(&mut msg, null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }
        unsafe {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    if let Some(worker) = &ui.worker {
        worker.queue.shutdown();
    }
    ui.icon(hwnd, NIM_DELETE);
    unsafe {
        UnregisterClassW(class.as_ptr(), module);
    }
    Ok(())
}
