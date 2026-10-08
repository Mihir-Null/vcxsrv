//! Public isolated configuration paths; production still has one manager per TokenUser SID.
use std::{ffi::OsString, path::PathBuf};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Tray,
    Doctor,
    Recover,
    Help,
}
#[derive(Clone)]
pub struct Options {
    pub mode: Mode,
    pub state_dir: Option<PathBuf>,
    pub config: Option<PathBuf>,
}
impl Options {
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, &'static str> {
        let mut args = args.into_iter();
        let mut mode = None;
        let mut json = false;
        let mut state_dir = None;
        let mut config = None;
        while let Some(arg) = args.next() {
            match arg.to_str() {
                Some("--tray") if mode.is_none() => mode = Some(Mode::Tray),
                Some("doctor") if mode.is_none() => mode = Some(Mode::Doctor),
                Some("recover-stopped") if mode.is_none() => mode = Some(Mode::Recover),
                Some("--help") | Some("-h") if mode.is_none() => mode = Some(Mode::Help),
                Some("--json") if !json => json = true,
                Some("--state-dir") if state_dir.is_none() => state_dir = Some(path(args.next())?),
                Some("--config") if config.is_none() => config = Some(path(args.next())?),
                _ => return Err("Unknown or duplicate argument; use --help"),
            }
        }
        let mode = mode.unwrap_or(Mode::Tray);
        if (mode == Mode::Doctor) != json {
            return Err("Use doctor --json for diagnostics");
        }
        Ok(Self {
            mode,
            state_dir,
            config,
        })
    }
    pub fn state_path(&self) -> Result<PathBuf, &'static str> {
        self.state_dir.clone().map(Ok).unwrap_or_else(default_state)
    }
    pub fn config_path(&self) -> Result<PathBuf, &'static str> {
        self.config
            .clone()
            .map(Ok)
            .unwrap_or_else(|| self.state_path().map(|p| p.join("config.json")))
    }
}
fn path(value: Option<OsString>) -> Result<PathBuf, &'static str> {
    let path = PathBuf::from(value.ok_or("Missing absolute path")?);
    if !path.is_absolute()
        || path
            .as_os_str()
            .to_string_lossy()
            .chars()
            .any(char::is_control)
    {
        return Err("Paths must be absolute local paths");
    }
    Ok(path)
}
fn default_state() -> Result<PathBuf, &'static str> {
    use std::{os::windows::ffi::OsStringExt, ptr::null_mut};
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath},
    };
    let mut p = null_mut();
    if unsafe { SHGetKnownFolderPath(&FOLDERID_LocalAppData, 0, null_mut(), &mut p) } < 0 {
        return Err("Windows LocalAppData is unavailable; use --state-dir");
    }
    let mut len = 0;
    unsafe {
        while *p.add(len) != 0 {
            len += 1;
        }
    }
    let path = PathBuf::from(unsafe { OsString::from_wide(std::slice::from_raw_parts(p, len)) });
    unsafe {
        CoTaskMemFree(p.cast());
    }
    Ok(path.join("VcXsrvWSL"))
}
