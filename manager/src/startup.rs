//! Opt-in per-user Run entry. An entry belonging to another installation is never overwritten.
use crate::{cli::Options, process::native::argument};
use std::{
    ffi::OsStr,
    path::Path,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS},
    System::Registry::*,
};
const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "VcXsrvWSL";
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            RegCloseKey(self.0);
        }
    }
}
pub fn command(exe: &Path, options: &Options) -> Result<Vec<u16>, &'static str> {
    let mut out = Vec::new();
    let state = options.state_path()?;
    let mut items = vec![
        exe.as_os_str(),
        OsStr::new("--tray"),
        OsStr::new("--state-dir"),
        state.as_os_str(),
    ];
    if let Some(config) = &options.config {
        items.extend([OsStr::new("--config"), config.as_os_str()]);
    }
    for item in items {
        if !out.is_empty() {
            out.push(32);
        }
        argument(item, &mut out).map_err(|_| "Invalid startup path")?;
    }
    out.push(0);
    Ok(out)
}
fn open(write: bool) -> Result<Option<Key>, &'static str> {
    let mut key = null_mut();
    let path = wide(KEY);
    let result = unsafe {
        if write {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                null(),
                0,
                KEY_QUERY_VALUE | KEY_SET_VALUE,
                null(),
                &mut key,
                null_mut(),
            )
        } else {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                KEY_QUERY_VALUE,
                &mut key,
            )
        }
    };
    if result == ERROR_FILE_NOT_FOUND {
        Ok(None)
    } else if result == ERROR_SUCCESS {
        Ok(Some(Key(key)))
    } else {
        Err("Startup registry access failed")
    }
}
fn read(key: &Key) -> Result<Option<Vec<u16>>, &'static str> {
    let name = wide(VALUE);
    let mut kind = 0;
    let mut size = 0;
    let first = unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            null_mut(),
            &mut size,
        )
    };
    if first == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    if first != ERROR_SUCCESS || kind != REG_SZ || size > 65536 || size % 2 != 0 {
        return Err("Startup entry is invalid or belongs to another installation");
    }
    let mut value = vec![0u16; size as usize / 2];
    if unsafe {
        RegQueryValueExW(
            key.0,
            name.as_ptr(),
            null(),
            &mut kind,
            value.as_mut_ptr().cast(),
            &mut size,
        )
    } != ERROR_SUCCESS
        || kind != REG_SZ
    {
        return Err("Startup entry changed");
    }
    value.truncate(size as usize / 2);
    Ok(Some(value))
}
pub fn enabled(exe: &Path, options: &Options) -> Result<bool, &'static str> {
    let Some(key) = open(false)? else {
        return Ok(false);
    };
    match read(&key)? {None=>Ok(false),Some(existing) if existing==command(exe,options)?=>Ok(true),Some(_)=>Err("Startup belongs to another installation; disable it there before enabling this manager")}
}
pub fn toggle(exe: &Path, options: &Options) -> Result<bool, &'static str> {
    let key = open(true)?.ok_or("Startup key unavailable")?;
    let expected = command(exe, options)?;
    let name = wide(VALUE);
    let enable = match read(&key)? {
        None => true,
        Some(existing) if existing == expected => false,
        Some(_) => return Err("Startup belongs to another installation; disable it there first"),
    };
    let result = unsafe {
        if enable {
            RegSetValueExW(
                key.0,
                name.as_ptr(),
                0,
                REG_SZ,
                expected.as_ptr().cast(),
                (expected.len() * 2) as u32,
            )
        } else {
            RegDeleteValueW(key.0, name.as_ptr())
        }
    };
    if result != ERROR_SUCCESS {
        Err("Startup registry update failed")
    } else {
        Ok(enable)
    }
}
