use std::path::{Path, PathBuf};
use vcxsrv_wsl_manager::{cli::Options, startup};

#[test]
fn startup_preserves_implicit_first_run_and_explicit_custom_config() {
    let exe = Path::new(r"C:\App with spaces\manager.exe");
    let implicit = Options::parse(
        ["--tray", "--state-dir", r"C:\Private state"].map(std::ffi::OsString::from),
    )
    .unwrap();
    let command = String::from_utf16_lossy(&startup::command(exe, &implicit).unwrap());
    assert!(!command.contains("--config"));
    let explicit = Options {
        config: Some(PathBuf::from(r"C:\Private state\custom.json")),
        ..implicit
    };
    let command = String::from_utf16_lossy(&startup::command(exe, &explicit).unwrap());
    assert!(command.contains(r#"--config "C:\Private state\custom.json""#));
}
