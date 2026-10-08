//! A handle-presence guard, not a thread-owned mutex. It may safely move between threads.
pub use crate::native::NativeError as InstanceError;
use crate::native::*;
use std::{ffi::OsStr, os::windows::io::OwnedHandle};
use windows_sys::Win32::{
    Foundation::*, Security::Authorization::SE_KERNEL_OBJECT, System::Threading::*,
};
pub struct InstanceGuard {
    _handle: OwnedHandle,
    sid: String,
}
impl InstanceGuard {
    /// Production uses scope `manager`; alternate scopes are reserved for isolated fixtures.
    pub fn acquire(scope: &str) -> Result<Self, InstanceError> {
        if scope.is_empty()
            || scope.len() > 100
            || !scope
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(InstanceError::InvalidName);
        }
        let sid = current_sid()?;
        let security = Security::for_sid(&sid)?;
        let name = wide(OsStr::new(&format!(r"Global\VcXsrvWSL.{scope}.{sid}")))?;
        let h = unsafe { CreateMutexW(&security.attributes(), 0, name.as_ptr()) };
        let code = unsafe { GetLastError() };
        let h = handle(h)?;
        validate_security(raw(&h), SE_KERNEL_OBJECT, &sid)?;
        if code == ERROR_ALREADY_EXISTS {
            return Err(InstanceError::AlreadyRunning);
        }
        Ok(Self { _handle: h, sid })
    }
    pub(crate) fn raw_handle(&self) -> usize {
        raw(&self._handle) as usize
    }
    pub fn sid(&self) -> &str {
        &self.sid
    }
}
