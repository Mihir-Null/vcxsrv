//! Fixed diagnostics only. Never construct status text from command output or credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    Deadline,
    Command,
    Protocol,
    UnsafeStorage,
    OwnershipMismatch,
    CredentialConflict,
    ServerExited,
    GuestStopped,
    MissingServer,
    MissingHelper,
    ServerStart,
    RecoveryRefused,
}
impl Failure {
    pub fn message(self) -> &'static str {
        match self {
            Self::Deadline => "Connection attempt timed out",
            Self::Command => "Command failed",
            Self::Protocol => "Invalid helper response",
            Self::UnsafeStorage => "Private storage failed validation",
            Self::OwnershipMismatch => "Server ownership could not be verified; existing or stale metadata requires explicit recovery, never automatic credential replacement",
            Self::CredentialConflict => "Existing credentials conflict; recovery required",
            Self::ServerExited => "Server exited",
            Self::MissingServer => "Companion vcxsrv.exe is missing; install it beside the manager or set server_path in config.json",
            Self::MissingHelper => "Guest helper unavailable; install vcxsrv-wsl in the configured distro and verify user settings",
            Self::RecoveryRefused => "Recovery refused: server may be alive, or metadata/authority/scope is unsafe. Close live clients only through confirmed Stop; no data was reset",
            Self::ServerStart => "Server could not start; use the normal desktop shortcut and verify companion, display and launcher job restrictions",
            Self::GuestStopped => "Selected WSL guest stopped during connection",
        }
    }
}
