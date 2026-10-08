//! Native callbacks retain no Ui references across message-pumping Windows APIs.
use crate::{
    cli::Options,
    controller::Action,
    native::wide,
    startup,
    worker::{ActionQueue, Snapshot, Worker},
};
use std::{
    ffi::OsStr,
    ptr::{null, null_mut},
    sync::Arc,
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
const KEY_SELECT: u32 = NIN_SELECT | NINF_KEY;
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
}
/// Owns the allocation until the outer message loop unwinds. WndProc only clears
/// GWLP_USERDATA; nested DestroyWindow cannot free state beneath a modal frame.
struct UiAllocation(*mut Ui);
impl Drop for UiAllocation {
    fn drop(&mut self) {
        unsafe {
            let ui = Box::from_raw(self.0);
            if let Some(worker) = &ui.worker {
                worker.queue.shutdown();
            }
            drop(ui);
        }
    }
}
struct UiSnapshot {
    options: Options,
    queue: Option<Arc<ActionQueue>>,
    latest: Option<Snapshot>,
}
impl UiSnapshot {
    fn status(&self) -> String {
        self.latest
            .as_ref()
            .map(Snapshot::status)
            .unwrap_or_else(|| "Starting manager".into())
    }
    fn busy(&self) -> bool {
        self.queue.as_ref().is_some_and(|queue| queue.busy())
    }
}
/// The only Ui borrow is this short shared read. All returned fields are owned
/// clones. Its mutex guard and Ui reference end before the caller invokes Win32.
unsafe fn snapshot(hwnd: HWND, expected: *mut Ui) -> Option<UiSnapshot> {
    let current = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
    if current.is_null() || current != expected {
        return None;
    }
    let ui = &*current;
    let latest = ui
        .worker
        .as_ref()
        .map(|worker| worker.latest.lock().unwrap().clone());
    Some(UiSnapshot {
        options: ui.options.clone(),
        queue: ui.worker.as_ref().map(|worker| worker.queue.clone()),
        latest,
    })
}
fn notify_icon(hwnd: HWND, message: u32, status: &str) -> bool {
    let mut icon: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    icon.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    icon.hWnd = hwnd;
    icon.uID = 1;
    icon.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
    icon.uCallbackMessage = CALLBACK;
    icon.hIcon = unsafe { LoadIconW(null_mut(), IDI_APPLICATION) };
    let tip = format!("VcXsrv WSL: {status}");
    let units: Vec<u16> = tip.encode_utf16().take(127).collect();
    icon.szTip[..units.len()].copy_from_slice(&units);
    if unsafe { Shell_NotifyIconW(message, &icon) } == 0 {
        return false;
    }
    if message == NIM_ADD {
        icon.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        (unsafe { Shell_NotifyIconW(NIM_SETVERSION, &icon) }) != 0
    } else {
        true
    }
}
fn menu_item(menu: HMENU, id: u32, text: &str, flags: u32) {
    if let Ok(text) = wide(OsStr::new(text)) {
        unsafe {
            AppendMenuW(menu, MF_STRING | flags, id as usize, text.as_ptr());
        }
    }
}
fn menu(hwnd: HWND, expected: *mut Ui) {
    let Some(state) = (unsafe { snapshot(hwnd, expected) }) else {
        return;
    };
    let popup = unsafe { CreatePopupMenu() };
    if popup.is_null() {
        return;
    }
    let busy = state.busy();
    menu_item(popup, 0, &state.status(), MF_DISABLED);
    for (id, text) in [
        (CMD_START, "Start server"),
        (CMD_LAUNCH, "Launch Emacs"),
        (CMD_REFRESH, "Refresh connection"),
    ] {
        menu_item(popup, id, text, if busy { MF_GRAYED } else { 0 });
    }
    unsafe {
        AppendMenuW(popup, MF_SEPARATOR, 0, null());
    }
    for (id, text) in [
        (CMD_DIAGNOSTICS, "Diagnostics"),
        (CMD_CONFIG, "Open config"),
        (CMD_LOGS, "Open logs"),
    ] {
        menu_item(popup, id, text, 0);
    }
    let enabled = std::env::current_exe()
        .ok()
        .is_some_and(|exe| startup::enabled(&exe, &state.options).unwrap_or(false));
    menu_item(
        popup,
        CMD_STARTUP,
        "Start manager at sign-in",
        if enabled { MF_CHECKED } else { 0 },
    );
    unsafe {
        AppendMenuW(popup, MF_SEPARATOR, 0, null());
    }
    menu_item(
        popup,
        CMD_RECOVER,
        "Recover stopped server record",
        if busy { MF_GRAYED } else { 0 },
    );
    menu_item(
        popup,
        CMD_STOP,
        "Stop server…",
        if busy { MF_GRAYED } else { 0 },
    );
    menu_item(popup, CMD_EXIT, "Exit manager (leave server running)", 0);
    let mut point: POINT = unsafe { std::mem::zeroed() };
    unsafe {
        GetCursorPos(&mut point);
        SetForegroundWindow(hwnd);
    }
    // state contains owned values only: no Ui reference/RefCell/mutex guard spans this pump.
    let choice = unsafe {
        TrackPopupMenu(
            popup,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            hwnd,
            null(),
        )
    } as u32;
    unsafe {
        DestroyMenu(popup);
    }
    // The owner may have been destroyed by a nested message. Recheck its exact context.
    if unsafe { snapshot(hwnd, expected) }.is_none() {
        return;
    }
    unsafe {
        PostMessageW(hwnd, WM_NULL, 0, 0);
    }
    if choice != 0 {
        command(hwnd, expected, choice);
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
fn command(hwnd: HWND, expected: *mut Ui, id: u32) {
    if unsafe { snapshot(hwnd, expected) }.is_none() {
        return;
    }
    if id == CMD_EXIT {
        let Some(state) = (unsafe { snapshot(hwnd, expected) }) else {
            return;
        };
        if let Some(queue) = state.queue {
            queue.shutdown();
        }
        notify_icon(hwnd, NIM_DELETE, "");
        unsafe {
            DestroyWindow(hwnd);
        }
        return;
    }
    let confirmed = if id == CMD_STOP {
        // All dialog inputs are independent owned buffers. No Ui view spans MessageBoxW.
        let text=wide(OsStr::new("Stop the owned display server? Connected X11 applications will disconnect. Exit manager leaves it running.")).unwrap();
        let title = wide(OsStr::new("Confirm Stop server")).unwrap();
        (unsafe {
            MessageBoxW(
                hwnd,
                text.as_ptr(),
                title.as_ptr(),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            )
        }) == IDYES
    } else {
        false
    };
    // Reacquire after the modal API; a nested Exit clears userdata and prevents dispatch.
    let Some(state) = (unsafe { snapshot(hwnd, expected) }) else {
        return;
    };
    if let Some(action) = command_action(id, confirmed) {
        if let Some(queue) = state.queue {
            queue.submit(action);
        }
        return;
    }
    match id {
        CMD_DIAGNOSTICS => {
            if let Some(s) = state.latest {
                let text=format!("{}\nCompanion present: {}\nValidated authority held: {}\nOwned server held: {}\nReady is the last authenticated observation; diagnostics do not poll the guest.",s.status(),s.companion_present,s.authority_valid,s.ownership_present);
                message(hwnd, &text);
            }
        }
        CMD_CONFIG | CMD_LOGS => {
            let path = if id == CMD_CONFIG {
                state.options.config_path()
            } else {
                state.options.state_path().map(|p| p.join("manager.log"))
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
                        if result as usize <= 32 && unsafe { snapshot(hwnd, expected) }.is_some() {
                            message(hwnd,"Could not open the private file; check its path and Windows file association");
                        }
                    }
                }
                Err(error) => message(hwnd, error),
            }
        }
        CMD_STARTUP => match std::env::current_exe()
            .map_err(|_| "Manager path unavailable")
            .and_then(|exe| startup::toggle(&exe, &state.options))
        {
            Ok(_) => {}
            Err(error) => message(hwnd, error),
        },
        _ => {}
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
    // Read the scalar through the raw context; never create &mut Ui in a callback.
    if message == (*pointer).taskbar_created {
        let Some(state) = snapshot(hwnd, pointer) else {
            return 0;
        };
        let status = state.status();
        notify_icon(hwnd, NIM_DELETE, &status);
        return notify_icon(hwnd, NIM_ADD, &status) as isize;
    }
    match message {
        CALLBACK
            if matches!(
                (lparam as u32) & 0xffff,
                WM_CONTEXTMENU | WM_RBUTTONUP | NIN_SELECT | KEY_SELECT
            ) =>
        {
            menu(hwnd, pointer);
            0
        }
        WORKER_UPDATE => {
            if let Some(state) = snapshot(hwnd, pointer) {
                notify_icon(hwnd, NIM_MODIFY, &state.status());
            }
            0
        }
        WM_COMMAND => {
            command(hwnd, pointer, (wparam & 0xffff) as u32);
            0
        }
        WM_CLOSE => {
            command(hwnd, pointer, CMD_EXIT);
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
    let taskbar_created =
        unsafe { RegisterWindowMessageW(wide(OsStr::new("TaskbarCreated")).unwrap().as_ptr()) };
    // Raw-owned allocation avoids a Box-derived mutable reference during window callbacks.
    let state = UiAllocation(Box::into_raw(Box::new(Ui {
        worker: None,
        options: options.clone(),
        taskbar_created,
    })));
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
            state.0.cast(),
        )
    };
    if hwnd.is_null() {
        unsafe {
            UnregisterClassW(class.as_ptr(), module);
        }
        return Err("Could not create native tray window");
    }
    let address = hwnd as usize;
    match Worker::start(options, move || unsafe {
        PostMessageW(address as HWND, WORKER_UPDATE, 0, 0);
    }) {
        Ok(worker) => unsafe {
            (*state.0).worker = Some(worker);
        },
        Err(error) => {
            unsafe {
                DestroyWindow(hwnd);
                UnregisterClassW(class.as_ptr(), module);
            }
            return Err(error);
        }
    }
    let initial =
        unsafe { snapshot(hwnd, state.0) }.ok_or("Tray window closed during initialization")?;
    if !notify_icon(hwnd, NIM_ADD, &initial.status()) {
        if let Some(queue) = initial.queue {
            queue.shutdown();
        }
        unsafe {
            DestroyWindow(hwnd);
            UnregisterClassW(class.as_ptr(), module);
        }
        return Err("Windows notification area is unavailable");
    }
    if let Some(queue) = initial.queue {
        queue.submit(Action::StartServer);
    }
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
    if let Some(snapshot) = unsafe { snapshot(hwnd, state.0) } {
        if let Some(queue) = snapshot.queue {
            queue.shutdown();
        }
    }
    notify_icon(hwnd, NIM_DELETE, "");
    if unsafe { IsWindow(hwnd) } != 0 {
        unsafe {
            DestroyWindow(hwnd);
        }
    }
    unsafe {
        UnregisterClassW(class.as_ptr(), module);
    }
    Ok(())
}
