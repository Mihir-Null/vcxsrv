//! Fixed read-only installer handshake. The parent pulls a mutex duplicate before RELEASE.
use crate::{
    config::Config,
    instance::InstanceGuard,
    native::*,
    owner::ServerIdentity,
    recovery::inspect_stopped_record,
    storage::{file_identity, open_shared, FileIdentity, ProtectedDirectory, StorageError},
};
use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Write},
    path::{Component, Path, PathBuf, Prefix},
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{Diagnostics::ToolHelp::*, Threading::*},
};
fn checked_root(path: &Path) -> Result<(Vec<File>, Option<FileIdentity>), &'static str> {
    let c: Vec<_> = path.components().collect();
    if c.len() < 3
        || !matches!(c[0],Component::Prefix(p) if matches!(p.kind(),Prefix::Disk(_)))
        || c[1] != Component::RootDir
        || c[2..].iter().any(|p| !matches!(p, Component::Normal(_)))
        || path.to_string_lossy().chars().any(char::is_control)
    {
        return Err("Installation root must be an absolute local directory without traversal");
    }
    let mut p = PathBuf::new();
    p.push(c[0]);
    p.push(c[1]);
    let mut held = Vec::new();
    let mut final_id = None;
    for part in std::iter::once(None).chain(c[2..].iter().map(Some)) {
        if let Some(part) = part {
            p.push(part);
        }
        match open_shared(
            &p,
            GENERIC_READ | READ_CONTROL,
            OPEN_EXISTING,
            None,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
        ) {
            Ok(f) => {
                let id = file_identity(&f, true).map_err(|_| "Installation ancestor is unsafe")?;
                if p == path {
                    final_id = Some(id);
                }
                held.push(f);
            }
            Err(StorageError::Missing) => break,
            Err(_) => return Err("Installation root cannot be inspected safely"),
        }
    }
    Ok((held, final_id))
}
fn normalized(path: &Path) -> String {
    path.to_string_lossy()
        .strip_prefix(r"\\?\")
        .unwrap_or(&path.to_string_lossy())
        .replace('/', "\\")
        .to_lowercase()
}
fn live_install_process(root: &Path, deadline: Instant) -> Result<(), &'static str> {
    let prefix = normalized(root) + "\\";
    let snapshot = handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })
        .map_err(|_| "Process inventory unavailable")?;
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
    let mut ok = unsafe { Process32FirstW(raw(&snapshot), &mut entry) };
    loop {
        if Instant::now() >= deadline {
            return Err("Installation inspection timed out");
        }
        if ok == 0 {
            if unsafe { GetLastError() } != ERROR_NO_MORE_FILES {
                return Err("Process inventory incomplete");
            }
            break;
        }
        let h = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                entry.th32ProcessID,
            )
        };
        // Unrelated inaccessible OS processes are not installation candidates.
        if !h.is_null() {
            let process = handle(h).map_err(|_| "Process handle unavailable")?;
            let mut name = vec![0u16; 32768];
            let mut n = name.len() as u32;
            if unsafe { QueryFullProcessImageNameW(raw(&process), 0, name.as_mut_ptr(), &mut n) }
                != 0
            {
                name.truncate(n as usize);
                if let Ok(name) = String::from_utf16(&name) {
                    if normalized(Path::new(&name)).starts_with(&prefix)
                        && unsafe { WaitForSingleObject(raw(&process), 0) } != WAIT_OBJECT_0
                    {
                        return Err("Installation has a running process; stop it explicitly before maintenance");
                    }
                }
            }
        }
        ok = unsafe { Process32NextW(raw(&snapshot), &mut entry) };
    }
    Ok(())
}
/// Alternate scopes are only for native fixtures; production CLI hardcodes manager.
pub fn serve(root: &Path, state: &Path, scope: &str) -> Result<(), &'static str> {
    let deadline = Instant::now() + Duration::from_secs(15);
    let guard = Arc::new(
        InstanceGuard::acquire(scope)
            .map_err(|_| "Close the running manager before installation maintenance")?,
    );
    let (_root_handles, root_id) = checked_root(root)?;
    let directory = match ProtectedDirectory::open_existing(state, guard.clone()) {
        Ok(d) => Some(d),
        Err(StorageError::Missing) => None,
        Err(_) => return Err("State failed private-storage validation; preserved"),
    };
    let target = root.join("vcxsrv-wsl-manager.exe");
    let config = if let Some(d) = directory.as_ref() {
        match d.read("config.json", 65536) {
            Ok(f) => Config::from_json(
                std::str::from_utf8(f.bytes()).map_err(|_| "Configuration must be UTF-8")?,
                &target,
            )
            .map_err(|_| "Configuration is invalid; preserved")?,
            Err(StorageError::Missing) => {
                Config::defaults_for(&target).map_err(|_| "Invalid installation defaults")?
            }
            Err(_) => return Err("Configuration failed private validation; preserved"),
        }
    } else {
        Config::defaults_for(&target).map_err(|_| "Invalid installation defaults")?
    };
    let proof=directory.as_ref().map(|d|inspect_stopped_record(d,&config,deadline)).transpose().map_err(|_|"Owned server is live or stopped evidence is unsafe; use confirmed Stop or explicit recover-stopped")?.flatten();
    if let Some(proof) = proof.as_ref() {
        let record: ServerIdentity =
            serde_json::from_slice(proof.record.bytes()).map_err(|_| "Invalid ownership record")?;
        if Some(record.installation) != root_id
            || normalized(record.image.parent().ok_or("Invalid owned installation")?)
                != normalized(root)
            || normalized(&record.image) != normalized(&config.server_path)
        {
            return Err("Ownership belongs to a different installation; preserved");
        }
    }
    live_install_process(root, deadline)?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = std::io::stdin()
            .take(17)
            .read_to_end(&mut bytes)
            .map(|_| bytes);
        let _ = sender.send(result);
    });
    println!(
        "{}",
        serde_json::json!({"version":1,"ready":true,"mutex_handle":format!("0x{:x}",guard.raw_handle()),"ownership":if proof.is_some(){"stopped"}else{"none"}})
    );
    std::io::stdout()
        .flush()
        .map_err(|_| "Installer handshake output failed")?;
    let remaining = deadline.saturating_duration_since(Instant::now());
    let bytes = receiver
        .recv_timeout(remaining)
        .map_err(|_| "Installer handshake timed out")?
        .map_err(|_| "Installer handshake input failed")?;
    if bytes != b"RELEASE\n" && bytes != b"RELEASE\r\n" {
        return Err("Installer handshake requires fixed RELEASE and EOF");
    }
    Ok(())
}
pub fn cli(args: impl IntoIterator<Item = OsString>) -> Result<(), &'static str> {
    let mut args = args.into_iter();
    let mut root = None;
    let mut state = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--root") if root.is_none() => {
                root = Some(PathBuf::from(args.next().ok_or("Missing root")?))
            }
            Some("--state-dir") if state.is_none() => {
                state = Some(PathBuf::from(args.next().ok_or("Missing state")?))
            }
            _ => {
                return Err("Use installation-guard --root ABSOLUTE_DIR [--state-dir ABSOLUTE_DIR]")
            }
        }
    }
    let state = state.unwrap_or(
        crate::cli::Options {
            mode: crate::cli::Mode::Doctor,
            state_dir: None,
            config: None,
        }
        .state_path()?,
    );
    serve(
        &root.ok_or("Installation root is required")?,
        &state,
        "manager",
    )
}
