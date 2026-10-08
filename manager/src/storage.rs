//! Strict private storage: SID-only protected DACL, no reparse components, retained
//! ancestor handles without delete sharing, bounded reads and atomic publication.
pub use crate::native::NativeError as StorageError;
use crate::{instance::InstanceGuard, native::*};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::{Component, Path, PathBuf, Prefix},
    ptr::{null, null_mut},
    sync::Arc,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::SE_FILE_OBJECT, Cryptography::*},
    Storage::FileSystem::*,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileIdentity {
    pub volume: u32,
    pub index: u64,
}
pub struct ProtectedFile {
    file: File,
    identity: FileIdentity,
    bytes: Vec<u8>,
}
impl ProtectedFile {
    pub fn identity(&self) -> FileIdentity {
        self.identity
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn path_handle(&self) -> &File {
        &self.file
    }
}
pub struct ProtectedDirectory {
    path: PathBuf,
    _ancestors: Vec<File>,
    guard: Option<Arc<InstanceGuard>>,
    sid: String,
    identity: FileIdentity,
}
fn io_error(e: std::io::Error) -> StorageError {
    StorageError::Native(e.raw_os_error().unwrap_or(0) as u32)
}
pub(crate) fn open_file(
    path: &Path,
    access: u32,
    creation: u32,
    security: Option<&Security>,
) -> Result<File, StorageError> {
    open_shared(path, access, creation, security, FILE_SHARE_READ)
}
pub(crate) fn open_shared(
    path: &Path,
    access: u32,
    creation: u32,
    security: Option<&Security>,
    share: u32,
) -> Result<File, StorageError> {
    let p = wide(path.as_os_str())?;
    let sa = security.map(Security::attributes);
    let h = unsafe {
        CreateFileW(
            p.as_ptr(),
            access,
            share,
            sa.as_ref().map_or(null(), |s| s),
            creation,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        let code = unsafe { GetLastError() };
        return Err(
            if code == ERROR_FILE_NOT_FOUND || code == ERROR_PATH_NOT_FOUND {
                StorageError::Missing
            } else {
                StorageError::Native(code)
            },
        );
    }
    Ok(unsafe { File::from_raw_handle(h) })
}
pub(crate) fn file_identity(file: &File, directory: bool) -> Result<FileIdentity, StorageError> {
    unsafe {
        let mut info: BY_HANDLE_FILE_INFORMATION = std::mem::zeroed();
        if GetFileInformationByHandle(file.as_raw_handle(), &mut info) == 0 {
            return Err(last());
        }
        if GetFileType(file.as_raw_handle()) != FILE_TYPE_DISK
            || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || (info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0) != directory
            || (!directory && info.nNumberOfLinks != 1)
        {
            return Err(StorageError::Unsafe);
        }
        Ok(FileIdentity {
            volume: info.dwVolumeSerialNumber,
            index: ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64,
        })
    }
}
fn filename(name: &str) -> Result<(), StorageError> {
    if name.is_empty()
        || name.len() > 120
        || name == "."
        || name == ".."
        || name.ends_with('.')
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    {
        Err(StorageError::InvalidName)
    } else {
        Ok(())
    }
}
impl ProtectedDirectory {
    pub(crate) fn require_guard(&self) -> Result<(), StorageError> {
        if self.guard.is_some() {
            Ok(())
        } else {
            Err(StorageError::Unsafe)
        }
    }
    pub fn open_or_create(path: &Path, guard: Arc<InstanceGuard>) -> Result<Self, StorageError> {
        Self::open(path, Some(guard))
    }
    pub fn inspect(path: &Path) -> Result<Self, StorageError> {
        Self::open(path, None)
    }
    fn open(path: &Path, guard: Option<Arc<InstanceGuard>>) -> Result<Self, StorageError> {
        let c: Vec<_> = path.components().collect();
        if c.len() < 3
            || !matches!(c[0],Component::Prefix(p) if matches!(p.kind(),Prefix::Disk(_)))
            || c[1] != Component::RootDir
            || c[2..].iter().any(|x| !matches!(x, Component::Normal(_)))
        {
            return Err(StorageError::InvalidName);
        }
        let sid = current_sid()?;
        let security = Security::for_sid(&sid)?;
        let mut p = PathBuf::new();
        let mut ancestors = Vec::new();
        p.push(c[0]);
        p.push(c[1]);
        let root = open_shared(
            &p,
            GENERIC_READ | READ_CONTROL,
            OPEN_EXISTING,
            None,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
        )?;
        file_identity(&root, true)?;
        ancestors.push(root);
        for (i, component) in c.iter().enumerate().skip(2) {
            p.push(component);
            if i == c.len() - 1 && guard.is_some() {
                let w = wide(p.as_os_str())?;
                if unsafe { CreateDirectoryW(w.as_ptr(), &security.attributes()) } == 0
                    && unsafe { GetLastError() } != ERROR_ALREADY_EXISTS
                {
                    return Err(last());
                }
            }
            let f = open_shared(
                &p,
                GENERIC_READ | READ_CONTROL,
                OPEN_EXISTING,
                None,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
            )?;
            file_identity(&f, true)?;
            ancestors.push(f);
        }
        let last = ancestors.last().unwrap();
        validate_security(last.as_raw_handle(), SE_FILE_OBJECT, &sid)?;
        let identity = file_identity(last, true)?;
        Ok(Self {
            path: p,
            _ancestors: ancestors,
            guard,
            sid,
            identity,
        })
    }
    pub fn identity(&self) -> FileIdentity {
        self.identity
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn sid(&self) -> &str {
        &self.sid
    }
    fn validate(&self, file: &File) -> Result<FileIdentity, StorageError> {
        validate_security(file.as_raw_handle(), SE_FILE_OBJECT, self.sid())?;
        file_identity(file, false)
    }
    pub fn read(&self, name: &str, limit: usize) -> Result<ProtectedFile, StorageError> {
        filename(name)?;
        let mut file = open_file(
            &self.path.join(name),
            GENERIC_READ | READ_CONTROL,
            OPEN_EXISTING,
            None,
        )?;
        let identity = self.validate(&file)?;
        if file.metadata().map_err(io_error)?.len() > limit as u64 {
            return Err(StorageError::TooLarge);
        }
        let mut bytes = Vec::new();
        (&mut file)
            .take(limit.saturating_add(1) as u64)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        if bytes.len() > limit {
            return Err(StorageError::TooLarge);
        }
        Ok(ProtectedFile {
            file,
            identity,
            bytes,
        })
    }
    pub fn create_new(&self, name: &str, bytes: &[u8]) -> Result<ProtectedFile, StorageError> {
        self.publish(name, bytes, false, |file| self.validate(file))?;
        self.read(name, bytes.len())
    }
    /// Atomic replacement under the lifetime instance guard. Existing destinations must
    /// already be safe; an unsafe object is never repaired or overwritten.
    /// Final validation can fail after commit; the published file is preserved for recovery.
    pub fn replace(&self, name: &str, bytes: &[u8]) -> Result<(), StorageError> {
        self.publish(name, bytes, true, |file| self.validate(file))
    }
    fn publish(
        &self,
        name: &str,
        bytes: &[u8],
        replace: bool,
        validate_published: impl FnOnce(&File) -> Result<FileIdentity, StorageError>,
    ) -> Result<(), StorageError> {
        self.require_guard()?;
        if bytes.len() > 1024 * 1024 {
            return Err(StorageError::TooLarge);
        }
        filename(name)?;
        match self.read(name, 1024 * 1024) {
            Ok(_) => {}
            Err(StorageError::Missing) => {}
            Err(e) => return Err(e),
        }
        let mut random = [0u8; 16];
        if unsafe {
            BCryptGenRandom(
                null_mut(),
                random.as_mut_ptr(),
                16,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        } < 0
        {
            return Err(StorageError::Unsafe);
        }
        let temp = format!(
            "tmp-{}",
            random
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let security = Security::for_sid(self.sid())?;
        let mut file = open_file(
            &self.path.join(&temp),
            GENERIC_READ | GENERIC_WRITE | DELETE | READ_CONTROL,
            CREATE_NEW,
            Some(&security),
        )?;
        self.validate(&file)?;
        let result = (|| {
            file.write_all(bytes).map_err(io_error)?;
            file.sync_all().map_err(io_error)?;
            // Revalidate immediately before replacement. Other users cannot change children in
            // this SID-only directory; same-user malicious code is outside the security boundary.
            match self.read(name, 1024 * 1024) {
                Ok(_) => {}
                Err(StorageError::Missing) => {}
                Err(e) => return Err(e),
            }
            let wide = wide(self.path.join(name).as_os_str())?;
            let n = std::mem::offset_of!(FILE_RENAME_INFO, FileName) + wide.len() * 2;
            let mut buffer = vec![0usize; n.div_ceil(size_of::<usize>())];
            unsafe {
                let info = buffer.as_mut_ptr().cast::<FILE_RENAME_INFO>();
                (*info).Anonymous.ReplaceIfExists = replace;
                (*info).RootDirectory = null_mut();
                (*info).FileNameLength = ((wide.len() - 1) * 2) as u32;
                std::ptr::copy_nonoverlapping(
                    wide.as_ptr(),
                    (*info).FileName.as_mut_ptr(),
                    wide.len(),
                );
                if SetFileInformationByHandle(
                    file.as_raw_handle(),
                    FileRenameInfo,
                    info.cast(),
                    n as u32,
                ) == 0
                {
                    return Err(last());
                }
            }
            Ok(())
        })();
        if result.is_err() {
            let info = FILE_DISPOSITION_INFO { DeleteFile: true };
            unsafe {
                SetFileInformationByHandle(
                    file.as_raw_handle(),
                    FileDispositionInfo,
                    (&info as *const FILE_DISPOSITION_INFO).cast(),
                    size_of_val(&info) as u32,
                );
            }
        }
        result?;
        // Rename is the commit point: this handle now names the destination.
        // A failed final check reports uncertainty but must never delete it.
        validate_published(&file)?;
        Ok(())
    }
    pub fn remove(&self, name: &str, expected: FileIdentity) -> Result<(), StorageError> {
        self.require_guard()?;
        filename(name)?;
        let file = open_file(
            &self.path.join(name),
            DELETE | READ_CONTROL | FILE_READ_ATTRIBUTES,
            OPEN_EXISTING,
            None,
        )?;
        if self.validate(&file)? != expected {
            return Err(StorageError::OwnershipMismatch);
        }
        let info = FILE_DISPOSITION_INFO { DeleteFile: true };
        if unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FileDispositionInfo,
                (&info as *const FILE_DISPOSITION_INFO).cast(),
                size_of_val(&info) as u32,
            )
        } == 0
        {
            return Err(last());
        }
        Ok(())
    }
}

#[cfg(test)]
mod publication_tests {
    use super::*;

    #[test]
    fn post_rename_validation_error_preserves_committed_destination() {
        for replace in [false, true] {
            let scope = format!("post-publish-{}-{replace}", std::process::id());
            let path = std::env::temp_dir().join(&scope);
            let guard = Arc::new(InstanceGuard::acquire(&scope).unwrap());
            let directory = ProtectedDirectory::open_or_create(&path, guard).unwrap();
            if replace {
                drop(directory.create_new("authority", b"original").unwrap());
            }
            let mut published_identity = None;
            let result = directory.publish("authority", b"replacement", replace, |file| {
                // The real native rename must already have committed before this
                // injected failure of the final identity/security query.
                assert_eq!(
                    std::fs::read(path.join("authority")).unwrap(),
                    b"replacement"
                );
                published_identity = Some(file_identity(file, false).unwrap());
                Err(StorageError::Native(ERROR_ACCESS_DENIED))
            });
            assert_eq!(result, Err(StorageError::Native(ERROR_ACCESS_DENIED)));
            let observed = directory.read("authority", 64);
            let preserved = observed.as_ref().is_ok_and(|file| {
                file.bytes() == b"replacement" && Some(file.identity()) == published_identity
            });
            let entries = std::fs::read_dir(&path).unwrap().count();
            drop(observed);
            drop(directory);
            std::fs::remove_dir_all(path).unwrap();
            assert!(
                preserved,
                "post-rename verification failure deleted published data"
            );
            assert_eq!(entries, 1, "unpublished temporary file was retained");
        }
    }
}
