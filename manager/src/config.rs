use serde::{Deserialize, Serialize};
use std::{
    fmt,
    path::{Path, PathBuf},
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchSpec {
    pub program: String,
    pub args: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub distro: String,
    pub user: String,
    pub display: u16,
    pub server_path: PathBuf,
    pub launcher: LaunchSpec,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    Schema,
    Distro,
    User,
    Display,
    ServerPath,
    Launcher,
    Json,
}
impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid configuration field: {self:?}")
    }
}
impl std::error::Error for ConfigError {}
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("version", &self.version)
            .field("display", &self.display)
            .finish_non_exhaustive()
    }
}
impl fmt::Debug for LaunchSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LaunchSpec { .. }")
    }
}
impl Config {
    pub fn defaults_for(manager_exe: &Path) -> Result<Self, ConfigError> {
        let parent = manager_exe
            .parent()
            .filter(|_| manager_exe.is_absolute())
            .ok_or(ConfigError::ServerPath)?;
        Ok(Self {
            version: 1,
            distro: "NixOS".into(),
            user: "Empty".into(),
            display: 1,
            server_path: parent.join("vcxsrv.exe"),
            launcher: LaunchSpec {
                program: "emacsclient".into(),
                args: vec!["-c".into(), "-n".into()],
            },
        })
    }
    pub fn vsock_port(&self) -> u32 {
        106000 + u32::from(self.display)
    }
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != 1 {
            return Err(ConfigError::Schema);
        }
        if !valid_name(&self.distro) {
            return Err(ConfigError::Distro);
        }
        if !valid_name(&self.user) {
            return Err(ConfigError::User);
        }
        if self.display > 99 {
            return Err(ConfigError::Display);
        }
        if !self.server_path.is_absolute()
            || self.server_path.as_os_str().is_empty()
            || self
                .server_path
                .to_string_lossy()
                .chars()
                .any(char::is_control)
        {
            return Err(ConfigError::ServerPath);
        }
        if self.launcher.program.trim().is_empty()
            || self.launcher.program.chars().any(char::is_control)
            || self
                .launcher
                .args
                .iter()
                .any(|s| s.chars().any(char::is_control))
        {
            return Err(ConfigError::Launcher);
        }
        Ok(())
    }
    pub fn from_json(json: &str, manager_exe: &Path) -> Result<Self, ConfigError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Stored {
            version: u32,
            distro: Option<String>,
            user: Option<String>,
            display: Option<u16>,
            server_path: Option<PathBuf>,
            launcher: Option<LaunchSpec>,
        }
        let s: Stored = serde_json::from_str(json).map_err(|_| ConfigError::Json)?;
        let d = Self::defaults_for(manager_exe)?;
        let c = Self {
            version: s.version,
            distro: s.distro.unwrap_or(d.distro),
            user: s.user.unwrap_or(d.user),
            display: s.display.unwrap_or(d.display),
            server_path: s.server_path.unwrap_or(d.server_path),
            launcher: s.launcher.unwrap_or(d.launcher),
        };
        c.validate()?;
        Ok(c)
    }
}

fn valid_name(s: &str) -> bool {
    !s.trim().is_empty() && s.encode_utf16().count() <= 256 && !s.chars().any(char::is_control)
}
