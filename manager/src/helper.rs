//! Trusted literal helper calls. Success JSON is separate from target program output.
use crate::{
    config::Config,
    process::{CommandSpec, ProcessError, ProcessRunner},
    wsl::system_wsl,
};
use serde::Deserialize;
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Duration};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperError {
    SystemPath,
    Protocol,
    Command(ProcessError),
    Missing,
    CredentialConflict,
    Rejected,
    Deadline,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperReply {
    Bootstrapped,
    Ready,
}
impl HelperReply {
    pub fn parse(bytes: &[u8], verb: &str, display: u16) -> Result<Self, HelperError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Reply {
            version: u32,
            verb: String,
            result: String,
            display: Option<u16>,
            port: Option<u32>,
            code: Option<String>,
        }
        if bytes.len() > 1024 || bytes.is_empty() {
            return Err(HelperError::Protocol);
        }
        let text = std::str::from_utf8(bytes).map_err(|_| HelperError::Protocol)?;
        let text = text
            .strip_suffix('\n')
            .unwrap_or(text)
            .strip_suffix('\r')
            .unwrap_or(text.strip_suffix('\n').unwrap_or(text));
        if text.contains(['\r', '\n']) {
            return Err(HelperError::Protocol);
        }
        let r: Reply = serde_json::from_str(text).map_err(|_| HelperError::Protocol)?;
        if r.version != 1 || r.verb != verb {
            return Err(HelperError::Protocol);
        }
        if r.result == "error" {
            let code = r
                .code
                .as_deref()
                .filter(|s| {
                    !s.is_empty()
                        && s.len() <= 64
                        && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
                })
                .ok_or(HelperError::Protocol)?;
            return Err(match code {
                "credential_conflict" => HelperError::CredentialConflict,
                "deadline" => HelperError::Deadline,
                _ => HelperError::Rejected,
            });
        }
        if r.display != Some(display)
            || r.port != Some(106000 + u32::from(display))
            || r.code.is_some()
        {
            return Err(HelperError::Protocol);
        }
        match (verb, r.result.as_str()) {
            ("bootstrap", "bootstrapped") => Ok(Self::Bootstrapped),
            ("probe", "ready") => Ok(Self::Ready),
            _ => Err(HelperError::Protocol),
        }
    }
}
pub struct HelperClient {
    config: Config,
    program: PathBuf,
}
impl HelperClient {
    pub fn new(config: Config) -> Result<Self, HelperError> {
        config.validate().map_err(|_| HelperError::Protocol)?;
        Ok(Self {
            config,
            program: system_wsl().map_err(|_| HelperError::SystemPath)?,
        })
    }
    pub fn spec(&self, verb: &str, timeout: Duration) -> Result<CommandSpec, HelperError> {
        if !matches!(verb, "bootstrap" | "probe" | "launch") {
            return Err(HelperError::Protocol);
        }
        let mut args = vec![
            "--distribution".into(),
            self.config.distro.clone().into(),
            "--user".into(),
            self.config.user.clone().into(),
            "--exec".into(),
            "/run/current-system/sw/bin/vcxsrv-wsl".into(),
            verb.into(),
        ];
        if verb == "launch" {
            args.push("--".into());
            args.push(self.config.launcher.program.clone().into());
            args.extend(self.config.launcher.args.iter().map(Into::into));
        }
        Ok(CommandSpec {
            program: self.program.clone(),
            args,
            timeout,
        })
    }
    pub fn invoke(
        &self,
        verb: &str,
        timeout: Duration,
        stdin: Option<&[u8]>,
        cancel: &AtomicBool,
    ) -> Result<Option<HelperReply>, HelperError> {
        let spec = self.spec(verb, timeout)?;
        let output = ProcessRunner::new(if verb == "launch" { 1024 * 1024 } else { 2048 })
            .run_cancellable(&spec, stdin, cancel)
            .map_err(|e| match e {
                ProcessError::Nonzero(127) => HelperError::Missing,
                ProcessError::Nonzero(6) => HelperError::CredentialConflict,
                ProcessError::Timeout | ProcessError::Nonzero(8) => HelperError::Deadline,
                _ => HelperError::Command(e),
            })?;
        if verb == "launch" {
            Ok(None)
        } else {
            HelperReply::parse(&output.stdout, verb, self.config.display).map(Some)
        }
    }
}
