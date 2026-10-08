use std::time::Duration;
use vcxsrv_wsl_manager::{
    process::ProcessRunner,
    wsl::{decode_running_distros, WslClient, WslError},
};
#[test]
fn decodes_exact_names_without_localized_status_assumptions() {
    let s = "\u{feff}NixOS\r\nDistro 空 🦀\r\n name with spaces \r\n";
    let b: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
    assert_eq!(
        decode_running_distros(&b).unwrap(),
        ["NixOS", "Distro 空 🦀", " name with spaces "]
    );
    assert_eq!(decode_running_distros(b"NixOS\n").unwrap(), ["NixOS"]);
    assert!(decode_running_distros(&[0xff, 0xfe, 0x00, 0xd8, 0x0a, 0x00]).is_err());
    assert_eq!(
        decode_running_distros(b"Bad\0Name\n"),
        Err(WslError::Output)
    );
    assert!(decode_running_distros(&[]).unwrap().is_empty());
}
#[test]
fn running_query_is_host_only_and_uses_system_executable() {
    let w = WslClient::new(ProcessRunner::default()).unwrap();
    let s = w.running_spec(Duration::from_secs(1));
    assert!(s.program.is_absolute());
    assert!(s
        .program
        .to_string_lossy()
        .to_ascii_lowercase()
        .ends_with(r"system32\wsl.exe"));
    assert_eq!(s.args, ["--list", "--running", "--quiet"]);
}
/// Real installed WSL host query; never supplies guest command/distro/exec switches.
#[test]
#[ignore = "explicit read-only installed WSL acceptance"]
fn installed_wsl_running_list() {
    let w = WslClient::new(ProcessRunner::default()).unwrap();
    let names = w.running_distros().unwrap();
    assert!(
        names.iter().any(|n| n == "NixOS"),
        "installed expected NixOS running membership missing"
    );
}
