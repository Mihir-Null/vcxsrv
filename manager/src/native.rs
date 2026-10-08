//! Shared handle/security primitives. Handles are never inheritable.
use std::{
    ffi::OsStr,
    os::windows::{
        ffi::OsStrExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    ptr::{addr_of, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::FILE_ALL_ACCESS,
    System::Threading::*,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeError {
    Native(u32),
    Unsafe,
    InvalidName,
    TooLarge,
    Missing,
    AlreadyRunning,
    OwnershipMismatch,
    Deadline,
    OuterJob,
    RecordExists,
}
impl std::fmt::Display for NativeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lifecycle operation failed: {self:?}")
    }
}
impl std::error::Error for NativeError {}
pub(crate) fn last() -> NativeError {
    NativeError::Native(unsafe { GetLastError() })
}
pub(crate) fn wide(s: &OsStr) -> Result<Vec<u16>, NativeError> {
    let mut s: Vec<u16> = s.encode_wide().collect();
    if s.contains(&0) {
        return Err(NativeError::InvalidName);
    }
    s.push(0);
    Ok(s)
}
pub(crate) fn handle(h: HANDLE) -> Result<OwnedHandle, NativeError> {
    if h.is_null() || h == INVALID_HANDLE_VALUE {
        Err(last())
    } else {
        Ok(unsafe { OwnedHandle::from_raw_handle(h) })
    }
}
pub(crate) fn raw(h: &OwnedHandle) -> HANDLE {
    h.as_raw_handle()
}
pub(crate) fn sid_of(process: HANDLE) -> Result<String, NativeError> {
    unsafe {
        let mut t = null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut t) == 0 {
            return Err(last());
        }
        let t = handle(t)?;
        let mut n = 0;
        GetTokenInformation(raw(&t), TokenUser, null_mut(), 0, &mut n);
        if n == 0 {
            return Err(last());
        }
        let mut data = vec![0usize; (n as usize).div_ceil(size_of::<usize>())];
        if GetTokenInformation(raw(&t), TokenUser, data.as_mut_ptr().cast(), n, &mut n) == 0 {
            return Err(last());
        }
        sid_string((*(data.as_ptr().cast::<TOKEN_USER>())).User.Sid)
    }
}
fn sid_string(sid: PSID) -> Result<String, NativeError> {
    unsafe {
        let mut p = null_mut();
        if ConvertSidToStringSidW(sid, &mut p) == 0 {
            return Err(last());
        }
        let mut n = 0;
        while *p.add(n) != 0 {
            n += 1
        }
        let text = String::from_utf16_lossy(std::slice::from_raw_parts(p, n));
        LocalFree(p.cast());
        Ok(text)
    }
}
pub(crate) fn current_sid() -> Result<String, NativeError> {
    sid_of(unsafe { GetCurrentProcess() })
}
pub(crate) struct Security(pub PSECURITY_DESCRIPTOR);
impl Security {
    pub fn for_sid(sid: &str) -> Result<Self, NativeError> {
        unsafe {
            let text = wide(OsStr::new(&format!("O:{sid}D:P(A;;GA;;;{sid})")))?;
            let mut sd = null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                null_mut(),
            ) == 0
            {
                return Err(last());
            }
            Ok(Self(sd))
        }
    }
    pub fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }
}
impl Drop for Security {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
pub(crate) fn validate_security(
    h: HANDLE,
    kind: SE_OBJECT_TYPE,
    sid: &str,
) -> Result<(), NativeError> {
    unsafe {
        let mut owner = null_mut();
        let mut acl = null_mut();
        let mut sd = null_mut();
        let r = GetSecurityInfo(
            h,
            kind,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut sd,
        );
        if r != 0 {
            return Err(NativeError::Native(r));
        }
        let _sd = Security(sd);
        if owner.is_null() || sid_string(owner)? != sid || acl.is_null() || (*acl).AceCount != 1 {
            return Err(NativeError::Unsafe);
        }
        let mut control = 0;
        let mut revision = 0;
        if GetSecurityDescriptorControl(sd, &mut control, &mut revision) == 0 {
            return Err(last());
        }
        if control & SE_DACL_PROTECTED == 0 {
            return Err(NativeError::Unsafe);
        }
        let mut ace = null_mut();
        if GetAce(acl, 0, &mut ace) == 0 {
            return Err(last());
        }
        let a = &*ace.cast::<ACCESS_ALLOWED_ACE>();
        let full = if kind == SE_FILE_OBJECT {
            FILE_ALL_ACCESS
        } else {
            MUTEX_ALL_ACCESS
        };
        if a.Header.AceType != 0
            || a.Header.AceFlags != 0
            || !(a.Mask == full || a.Mask == GENERIC_ALL)
            || sid_string(addr_of!(a.SidStart).cast_mut().cast())? != sid
        {
            return Err(NativeError::Unsafe);
        }
        Ok(())
    }
}
