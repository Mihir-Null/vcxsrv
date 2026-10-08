use crate::process::{CommandSpec, ProcessError, ProcessRunner};
use std::{fmt, path::PathBuf, time::Duration};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WslError {
    SystemPath,
    Command(ProcessError),
    Output,
}
impl fmt::Display for WslError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WSL query failed: {self:?}")
    }
}
impl std::error::Error for WslError {}
/// Passive membership queries only; these switches never select or execute in a guest.
pub struct WslClient {
    runner: ProcessRunner,
    program: PathBuf,
}
impl WslClient {
    pub fn new(runner: ProcessRunner) -> Result<Self, WslError> {
        Ok(Self {
            runner,
            program: system_wsl()?,
        })
    }
    pub fn running_spec(&self, timeout: Duration) -> CommandSpec {
        CommandSpec {
            program: self.program.clone(),
            args: vec!["--list".into(), "--running".into(), "--quiet".into()],
            timeout,
        }
    }
    pub fn running_distros(&self) -> Result<Vec<String>, WslError> {
        self.running_distros_with_timeout(Duration::from_secs(15))
    }
    pub fn running_distros_with_timeout(&self, timeout: Duration) -> Result<Vec<String>, WslError> {
        let output = self
            .runner
            .run(&self.running_spec(timeout), None)
            .map_err(WslError::Command)?;
        decode_running_distros(&output.stdout)
    }
}
pub(crate) fn system_wsl() -> Result<PathBuf, WslError> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt};
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
    // Initial supported manager is native x64, so System32 is not WOW64 redirected.
    let mut buffer = vec![0u16; 32768];
    let n = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if n == 0 || n >= buffer.len() {
        return Err(WslError::SystemPath);
    }
    buffer.truncate(n);
    let root = PathBuf::from(OsString::from_wide(&buffer));
    if !root.is_absolute() {
        return Err(WslError::SystemPath);
    }
    Ok(root.join("wsl.exe"))
}
/// WSL quiet output is UTF-16LE on the supported installation; UTF-8 variants are
/// accepted without parsing localized headings/status columns or trimming names.
pub fn decode_running_distros(bytes: &[u8]) -> Result<Vec<String>, WslError> {
    if bytes.len() > 1024 * 1024 {
        return Err(WslError::Output);
    }
    let text = if bytes.starts_with(&[0xff, 0xfe]) || bytes.contains(&0) {
        let b = bytes.strip_prefix(&[0xff, 0xfe]).unwrap_or(bytes);
        if !b.len().is_multiple_of(2) {
            return Err(WslError::Output);
        }
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&units).map_err(|_| WslError::Output)?
    } else {
        std::str::from_utf8(bytes)
            .map_err(|_| WslError::Output)?
            .to_owned()
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut names = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        if line.encode_utf16().count() > 256
            || line.chars().any(char::is_control)
            || line.trim().is_empty()
        {
            return Err(WslError::Output);
        }
        names.push(line.to_owned());
    }
    Ok(names)
}
