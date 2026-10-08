//! Explicit metadata retirement; positive stopped evidence never authorizes a process kill.
use crate::{
    authority::AuthorityStore,
    config::Config,
    native::{handle, raw},
    owner::ServerIdentity,
    storage::{ProtectedDirectory, StorageError},
};
use std::{
    path::{Component, Prefix},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    System::{Diagnostics::ToolHelp::*, Threading::*},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryResult {
    Retired,
    NoRecord,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryError {
    Storage(StorageError),
    Scope,
    Authority,
    StillAlive,
    UnknownProcess,
    Deadline,
}
fn budget(deadline: Instant) -> Result<(), RecoveryError> {
    if Instant::now() < deadline {
        Ok(())
    } else {
        Err(RecoveryError::Deadline)
    }
}
pub fn retire_stopped_record(
    store: &ProtectedDirectory,
    config: &Config,
    deadline: Instant,
) -> Result<RecoveryResult, RecoveryError> {
    store.require_guard().map_err(RecoveryError::Storage)?;
    budget(deadline)?;
    let record = match store.read("ownership.json", 16384) {
        Ok(file) => file,
        Err(StorageError::Missing) => return Ok(RecoveryResult::NoRecord),
        Err(e) => return Err(RecoveryError::Storage(e)),
    };
    let expected: ServerIdentity =
        serde_json::from_slice(record.bytes()).map_err(|_| RecoveryError::Scope)?;
    config.validate().map_err(|_| RecoveryError::Scope)?;
    let components: Vec<_> = expected.image.components().collect();
    if expected.version != 1
        || expected.pid == 0
        || expected.creation_time == 0
        || expected.sid != store.sid()
        || expected.storage != store.identity()
        || expected.distro != config.distro
        || expected.user != config.user
        || expected.display != config.display
        || expected.authority != "authority"
        || components.len() < 3
        || !matches!(components[0],Component::Prefix(p) if matches!(p.kind(),Prefix::Disk(_)|Prefix::VerbatimDisk(_)))
        || components[1] != Component::RootDir
        || components[2..]
            .iter()
            .any(|c| !matches!(c, Component::Normal(_)))
        || expected
            .image
            .to_string_lossy()
            .chars()
            .any(char::is_control)
    {
        return Err(RecoveryError::Scope);
    }
    // Load strictly; never generate or replace a credential, even after stopped evidence.
    let authority = AuthorityStore::new(store)
        .load(config.display)
        .map_err(|_| RecoveryError::Authority)?;
    if authority.file_identity() != Some(expected.authority_identity) {
        return Err(RecoveryError::Authority);
    }
    let evidence = stopped_process(&expected, deadline)?;
    budget(deadline)?;
    let identity = record.identity();
    let bytes = record.bytes().to_vec();
    drop(record);
    store
        .remove_if_unchanged("ownership.json", identity, &bytes)
        .map_err(RecoveryError::Storage)?;
    drop(evidence);
    drop(authority);
    Ok(RecoveryResult::Retired)
}
/// The optional retained handle pins a reused/signaled kernel object through deletion.
fn stopped_process(
    record: &ServerIdentity,
    deadline: Instant,
) -> Result<Option<std::os::windows::io::OwnedHandle>, RecoveryError> {
    budget(deadline)?;
    let h = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            record.pid,
        )
    };
    if h.is_null() {
        let code = unsafe { GetLastError() };
        if code != ERROR_INVALID_PARAMETER {
            return Err(RecoveryError::UnknownProcess);
        }
        let snapshot = handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })
            .map_err(|_| RecoveryError::UnknownProcess)?;
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = unsafe { Process32FirstW(raw(&snapshot), &mut entry) };
        let mut error = if ok == 0 {
            unsafe { GetLastError() }
        } else {
            0
        };
        loop {
            budget(deadline)?;
            if ok == 0 {
                if error != ERROR_NO_MORE_FILES {
                    return Err(RecoveryError::UnknownProcess);
                }
                break;
            }
            if entry.th32ProcessID == record.pid {
                return Err(RecoveryError::UnknownProcess);
            }
            ok = unsafe { Process32NextW(raw(&snapshot), &mut entry) };
            error = if ok == 0 {
                unsafe { GetLastError() }
            } else {
                0
            };
        }
        return Ok(None);
    }
    let handle = handle(h).map_err(|_| RecoveryError::UnknownProcess)?;
    if unsafe { GetProcessId(raw(&handle)) } != record.pid {
        return Err(RecoveryError::UnknownProcess);
    }
    let mut created: FILETIME = unsafe { std::mem::zeroed() };
    let mut exit = created;
    let mut kernel = created;
    let mut user = created;
    if unsafe {
        GetProcessTimes(
            raw(&handle),
            &mut created,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    } == 0
    {
        return Err(RecoveryError::UnknownProcess);
    }
    let creation = (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
    if creation != record.creation_time {
        return Ok(Some(handle));
    }
    match unsafe { WaitForSingleObject(raw(&handle), 0) } {
        WAIT_OBJECT_0 => Ok(Some(handle)),
        WAIT_TIMEOUT => Err(RecoveryError::StillAlive),
        _ => Err(RecoveryError::UnknownProcess),
    }
}
