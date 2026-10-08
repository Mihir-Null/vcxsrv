//! Owned persistent processes. No job object and no kill-on-drop behavior.
pub use crate::native::NativeError as OwnershipError;
use crate::{
    native::*,
    storage::{
        file_identity, open_file, open_shared, FileIdentity, ProtectedDirectory, ProtectedFile,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    fs::File,
    os::windows::io::OwnedHandle,
    path::{Component, PathBuf, Prefix},
    ptr::{null, null_mut},
    time::Instant,
};
use windows_sys::Win32::{
    Foundation::*,
    Storage::FileSystem::*,
    System::{JobObjects::*, Threading::*},
};
const RECORD: &str = "ownership.json";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerIdentity {
    pub version: u32,
    pub pid: u32,
    pub creation_time: u64,
    pub sid: String,
    pub image: PathBuf,
    pub image_identity: FileIdentity,
    pub installation: FileIdentity,
    pub storage: FileIdentity,
    pub distro: String,
    pub user: String,
    pub display: u16,
    pub authority: String,
    pub authority_identity: FileIdentity,
    /// Residual OS/launcher containment. This does not describe the ancestor job
    /// hierarchy or promise survival of external supervisor/session teardown.
    pub external_job: bool,
}
pub struct ServerBinding {
    image: PathBuf,
    image_file: File,
    installation_file: File,
    authority_file: ProtectedFile,
    distro: String,
    user: String,
    display: u16,
    authority: String,
    storage: FileIdentity,
    image_ancestors: Vec<File>,
}
impl ServerBinding {
    fn retained_files(&self) -> Result<Vec<File>, OwnershipError> {
        [
            &self.image_file,
            &self.installation_file,
            self.authority_file.path_handle(),
        ]
        .into_iter()
        .chain(self.image_ancestors.iter())
        .map(|f| {
            f.try_clone()
                .map_err(|e| OwnershipError::Native(e.raw_os_error().unwrap_or(0) as u32))
        })
        .collect()
    }
    /// Authority must already exist and pass Task6's record validation. This method never
    /// creates or rotates it, even if ownership metadata is missing or stale.
    pub fn new(
        store: &ProtectedDirectory,
        image: PathBuf,
        distro: String,
        user: String,
        display: u16,
        authority: &str,
    ) -> Result<Self, OwnershipError> {
        if !image.is_absolute() || display > 99 || distro.is_empty() || user.is_empty() {
            return Err(OwnershipError::InvalidName);
        }
        let image = std::fs::canonicalize(image).map_err(|_| OwnershipError::Unsafe)?;
        // Canonicalization selects a path; retained no-follow handles, not the path
        // string, prevent its ancestors being replaced during identity inspection.
        let components: Vec<_> = image.components().collect();
        if components.len() < 3
            || !matches!(components[0],Component::Prefix(p) if matches!(p.kind(),Prefix::Disk(_)|Prefix::VerbatimDisk(_)))
            || components[1] != Component::RootDir
        {
            return Err(OwnershipError::Unsafe);
        }
        let mut path = PathBuf::new();
        path.push(components[0]);
        path.push(components[1]);
        let mut image_ancestors = Vec::new();
        for component in
            std::iter::once(None).chain(components[2..components.len() - 1].iter().map(Some))
        {
            if let Some(component) = component {
                path.push(component)
            }
            let file = open_shared(
                &path,
                GENERIC_READ | READ_CONTROL,
                OPEN_EXISTING,
                None,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
            )?;
            file_identity(&file, true)?;
            image_ancestors.push(file);
        }
        let installation_file = open_shared(
            image.parent().ok_or(OwnershipError::Unsafe)?,
            GENERIC_READ | READ_CONTROL,
            OPEN_EXISTING,
            None,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
        )?;
        file_identity(&installation_file, true)?;
        let image_file = open_file(&image, GENERIC_READ | READ_CONTROL, OPEN_EXISTING, None)?;
        file_identity(&image_file, false)?;
        let authority_file = store.read(authority, 64)?;
        Ok(Self {
            image,
            image_file,
            installation_file,
            authority_file,
            distro,
            user,
            display,
            authority: authority.into(),
            storage: store.identity(),
            image_ancestors,
        })
    }
    fn identity(
        &self,
        store: &ProtectedDirectory,
        process: HANDLE,
    ) -> Result<ServerIdentity, OwnershipError> {
        if store.identity() != self.storage {
            return Err(OwnershipError::OwnershipMismatch);
        }
        let mut image = vec![0u16; 32768];
        let mut n = image.len() as u32;
        if unsafe { QueryFullProcessImageNameW(process, 0, image.as_mut_ptr(), &mut n) } == 0 {
            return Err(last());
        }
        image.truncate(n as usize);
        let actual = std::fs::canonicalize(PathBuf::from(
            String::from_utf16(&image).map_err(|_| OwnershipError::Unsafe)?,
        ))
        .map_err(|_| OwnershipError::Unsafe)?;
        if !actual
            .to_string_lossy()
            .eq_ignore_ascii_case(&self.image.to_string_lossy())
        {
            return Err(OwnershipError::OwnershipMismatch);
        }
        let mut created: FILETIME = unsafe { std::mem::zeroed() };
        let mut exit = created;
        let mut kernel = created;
        let mut user = created;
        if unsafe { GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user) } == 0
        {
            return Err(last());
        }
        let sid = sid_of(process)?;
        if sid != store.sid() {
            return Err(OwnershipError::OwnershipMismatch);
        }
        let mut external_job = 0;
        if unsafe { IsProcessInJob(process, null_mut(), &mut external_job) } == 0 {
            return Err(last());
        }
        Ok(ServerIdentity {
            version: 1,
            pid: unsafe { GetProcessId(process) },
            creation_time: ((created.dwHighDateTime as u64) << 32) | created.dwLowDateTime as u64,
            sid,
            image: self.image.clone(),
            image_identity: file_identity(&self.image_file, false)?,
            installation: file_identity(&self.installation_file, true)?,
            storage: store.identity(),
            distro: self.distro.clone(),
            user: self.user.clone(),
            display: self.display,
            authority: self.authority.clone(),
            authority_identity: self.authority_file.identity(),
            external_job: external_job != 0,
        })
    }
}
pub struct ServerOwner {
    process: OwnedHandle,
    identity: ServerIdentity,
    // Keep authority and executable identities locked even if the caller drops binding.
    _files: Vec<File>,
}
// A newly created, never-resumed process has no clients. Only this startup guard
// may clean it up on failure. Once resumed, ownership Drop always detaches.
struct SuspendedChild {
    process: Option<OwnedHandle>,
    thread: OwnedHandle,
}
impl Drop for SuspendedChild {
    fn drop(&mut self) {
        if let Some(process) = self.process.as_ref() {
            unsafe {
                TerminateProcess(raw(process), 1);
                WaitForSingleObject(raw(process), 1000);
            }
        }
    }
}
impl ServerOwner {
    pub fn identity(&self) -> &ServerIdentity {
        &self.identity
    }
    pub fn is_alive(&self) -> Result<bool, OwnershipError> {
        match unsafe { WaitForSingleObject(raw(&self.process), 0) } {
            WAIT_TIMEOUT => Ok(true),
            WAIT_OBJECT_0 => Ok(false),
            _ => Err(last()),
        }
    }
    pub fn spawn(
        store: &ProtectedDirectory,
        binding: &ServerBinding,
        args: &[OsString],
    ) -> Result<Self, OwnershipError> {
        store.require_guard()?;
        let retained_files = binding.retained_files()?;
        match store.read(RECORD, 16384) {
            Err(OwnershipError::Missing) => {}
            Ok(_) => return Err(OwnershipError::RecordExists),
            Err(e) => return Err(e),
        }
        let mut job = 0;
        if unsafe { IsProcessInJob(GetCurrentProcess(), null_mut(), &mut job) } == 0 {
            return Err(last());
        }
        // Break away if possible; failure leaves no child. DETACHED_PROCESS alone does not
        // remove inherited job membership. No manager-owned pipe handles reach this process.
        let flags = CREATE_NO_WINDOW
            | CREATE_SUSPENDED
            | if job != 0 {
                CREATE_BREAKAWAY_FROM_JOB
            } else {
                0
            };
        let app = wide(binding.image.as_os_str())?;
        let mut argv = Vec::new();
        crate::process::native::argument(binding.image.as_os_str(), &mut argv)
            .map_err(|_| OwnershipError::InvalidName)?;
        for arg in args {
            argv.push(32);
            crate::process::native::argument(arg, &mut argv)
                .map_err(|_| OwnershipError::InvalidName)?;
        }
        if argv.len() >= 32767 {
            return Err(OwnershipError::InvalidName);
        }
        argv.push(0);
        let mut info: STARTUPINFOW = unsafe { std::mem::zeroed() };
        info.cb = size_of::<STARTUPINFOW>() as u32;
        let mut created: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        // No inherited handles and no standard-stream pipes. CREATE_NO_WINDOW leaves
        // the console-less persistent child independent of the manager's console.
        if unsafe {
            CreateProcessW(
                app.as_ptr(),
                argv.as_mut_ptr(),
                null(),
                null(),
                0,
                flags,
                null(),
                null(),
                &info,
                &mut created,
            )
        } == 0
        {
            return Err(last());
        }
        let mut child = SuspendedChild {
            process: Some(handle(created.hProcess)?),
            thread: handle(created.hThread)?,
        };
        let process = child.process.as_ref().unwrap();
        // A successful breakaway can still leave OS-managed containment (for example
        // desktop activity jobs). Record it without claiming ancestor-job immunity.
        let identity = binding.identity(store, raw(process))?;
        let bytes = serde_json::to_vec(&identity).map_err(|_| OwnershipError::Unsafe)?;
        // Publish ownership before executing any server code. Startup failures destroy
        // only our never-resumed child and can never disconnect existing clients.
        let metadata = store.create_new(RECORD, &bytes)?;
        let record_identity = metadata.identity();
        drop(metadata);
        if unsafe { ResumeThread(raw(&child.thread)) } == u32::MAX {
            let error = last();
            let _ = store.remove(RECORD, record_identity);
            return Err(error);
        }
        let process = child.process.take().unwrap();
        Ok(Self {
            process,
            identity,
            _files: retained_files,
        })
    }
    pub fn recover(
        store: &ProtectedDirectory,
        binding: &ServerBinding,
    ) -> Result<Option<Self>, OwnershipError> {
        store.require_guard()?;
        let record = match store.read(RECORD, 16384) {
            Ok(r) => r,
            Err(OwnershipError::Missing) => return Ok(None),
            Err(e) => return Err(e),
        };
        let expected: ServerIdentity =
            serde_json::from_slice(record.bytes()).map_err(|_| OwnershipError::Unsafe)?;
        let query = handle(unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                0,
                expected.pid,
            )
        })?;
        if unsafe { WaitForSingleObject(raw(&query), 0) } != WAIT_TIMEOUT {
            return Err(OwnershipError::OwnershipMismatch);
        }
        let actual = binding.identity(store, raw(&query))?;
        if actual != expected {
            return Err(OwnershipError::OwnershipMismatch);
        }
        // Upgrade by duplicating the validated kernel object, never reopen a checked PID.
        let mut owned = null_mut();
        if unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                raw(&query),
                GetCurrentProcess(),
                &mut owned,
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                0,
                0,
            )
        } == 0
        {
            return Err(last());
        }
        Ok(Some(Self {
            process: handle(owned)?,
            identity: actual,
            _files: binding.retained_files()?,
        }))
    }
    /// Caller must obtain explicit user confirmation; ordinary Drop only closes handles.
    pub fn stop_confirmed(
        &mut self,
        store: &ProtectedDirectory,
        deadline: Instant,
    ) -> Result<(), OwnershipError> {
        store.require_guard()?;
        let record = store.read(RECORD, 16384)?;
        let expected: ServerIdentity =
            serde_json::from_slice(record.bytes()).map_err(|_| OwnershipError::Unsafe)?;
        if expected != self.identity || store.identity() != self.identity.storage {
            return Err(OwnershipError::OwnershipMismatch);
        }
        let record_identity = record.identity();
        drop(record);
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or(OwnershipError::Deadline)?;
        if self.is_alive()? && unsafe { TerminateProcess(raw(&self.process), 0) } == 0 {
            return Err(last());
        }
        match unsafe {
            WaitForSingleObject(
                raw(&self.process),
                remaining.as_millis().min(u32::MAX as u128 - 1) as u32,
            )
        } {
            WAIT_OBJECT_0 => store.remove(RECORD, record_identity),
            WAIT_TIMEOUT => Err(OwnershipError::Deadline),
            _ => Err(last()),
        }
    }
    pub fn detach(self) {}
}
