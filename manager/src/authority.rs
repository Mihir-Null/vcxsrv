//! One canonical Xauthority record; secret bytes never implement Debug or Display.
use crate::storage::{ProtectedDirectory, ProtectedFile, StorageError};
use std::{fmt, ptr::null_mut};
use windows_sys::Win32::Security::Cryptography::{
    BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityError {
    Display,
    Record,
    Random,
    Storage(StorageError),
    MissingForOwnedServer,
}
impl fmt::Display for AuthorityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "authority failed: {self:?}")
    }
}
impl std::error::Error for AuthorityError {}
pub struct Authority {
    display: u16,
    cookie: [u8; 16],
    _file: Option<ProtectedFile>,
}
impl Drop for Authority {
    fn drop(&mut self) {
        self.cookie.fill(0);
    }
}
impl Authority {
    pub fn decode_xauthority(bytes: &[u8], display: u16) -> Result<Self, AuthorityError> {
        if display > 99 {
            return Err(AuthorityError::Display);
        }
        let mut position = 0;
        fn number(bytes: &[u8], p: &mut usize) -> Result<u16, AuthorityError> {
            let pair = bytes
                .get(*p..p.saturating_add(2))
                .ok_or(AuthorityError::Record)?;
            *p += 2;
            Ok(u16::from_be_bytes([pair[0], pair[1]]))
        }
        fn field<'a>(bytes: &'a [u8], p: &mut usize) -> Result<&'a [u8], AuthorityError> {
            let length = number(bytes, p)? as usize;
            let data = bytes
                .get(*p..p.saturating_add(length))
                .ok_or(AuthorityError::Record)?;
            *p += length;
            Ok(data)
        }
        if bytes.len() > 64
            || number(bytes, &mut position)? != 65535
            || !field(bytes, &mut position)?.is_empty()
            || field(bytes, &mut position)? != display.to_string().as_bytes()
            || field(bytes, &mut position)? != b"MIT-MAGIC-COOKIE-1"
        {
            return Err(AuthorityError::Record);
        }
        let cookie: [u8; 16] = field(bytes, &mut position)?
            .try_into()
            .map_err(|_| AuthorityError::Record)?;
        if position != bytes.len() {
            return Err(AuthorityError::Record);
        }
        Ok(Self {
            display,
            cookie,
            _file: None,
        })
    }
    pub(crate) fn file_identity(&self) -> Option<crate::storage::FileIdentity> {
        self._file.as_ref().map(ProtectedFile::identity)
    }
    pub fn encode_xauthority(&self) -> Vec<u8> {
        let mut bytes = 65535u16.to_be_bytes().to_vec();
        for field in [
            &[][..],
            self.display.to_string().as_bytes(),
            b"MIT-MAGIC-COOKIE-1",
            &self.cookie,
        ] {
            bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
            bytes.extend_from_slice(field);
        }
        bytes
    }
}
pub struct AuthorityStore<'a> {
    directory: &'a ProtectedDirectory,
}
impl<'a> AuthorityStore<'a> {
    pub fn new(directory: &'a ProtectedDirectory) -> Self {
        Self { directory }
    }
    pub fn load(&self, display: u16) -> Result<Authority, AuthorityError> {
        let file = self
            .directory
            .read("authority", 64)
            .map_err(AuthorityError::Storage)?;
        let mut authority = Authority::decode_xauthority(file.bytes(), display)?;
        authority._file = Some(file);
        Ok(authority)
    }
    pub fn load_or_create(&self, display: u16) -> Result<Authority, AuthorityError> {
        if display > 99 {
            return Err(AuthorityError::Display);
        }
        match self.load(display) {
            Ok(authority) => return Ok(authority),
            Err(AuthorityError::Storage(StorageError::Missing)) => {}
            Err(error) => return Err(error),
        }
        match self.directory.read("ownership.json", 16384) {
            Err(StorageError::Missing) => {}
            Ok(_) => return Err(AuthorityError::MissingForOwnedServer),
            Err(error) => return Err(AuthorityError::Storage(error)),
        }
        let mut cookie = [0u8; 16];
        if unsafe {
            BCryptGenRandom(
                null_mut(),
                cookie.as_mut_ptr(),
                16,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        } < 0
        {
            return Err(AuthorityError::Random);
        }
        let mut authority = Authority {
            display,
            cookie,
            _file: None,
        };
        let mut bytes = authority.encode_xauthority();
        let publication = self.directory.create_new("authority", &bytes);
        bytes.fill(0);
        authority._file = Some(publication.map_err(AuthorityError::Storage)?);
        Ok(authority)
    }
}
