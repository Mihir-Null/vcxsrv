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
}
impl Failure {
    pub fn message(self) -> &'static str {
        match self {
            Self::Deadline => "Connection attempt timed out",
            Self::Command => "Command failed",
            Self::Protocol => "Invalid helper response",
            Self::UnsafeStorage => "Private storage failed validation",
            Self::OwnershipMismatch => "Server ownership could not be verified",
            Self::CredentialConflict => "Existing credentials conflict; recovery required",
            Self::ServerExited => "Server exited",
            Self::GuestStopped => "Selected WSL guest stopped during connection",
        }
    }
}
