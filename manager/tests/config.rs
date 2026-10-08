use std::path::Path;
use vcxsrv_wsl_manager::config::{Config, ConfigError};
#[test]
fn installed_defaults_and_port() {
    let c = Config::defaults_for(Path::new(r"C:\Programs\VcXsrvWSL\manager.exe")).unwrap();
    assert_eq!(
        (
            c.version,
            c.distro.as_str(),
            c.user.as_str(),
            c.display,
            c.vsock_port()
        ),
        (1, "NixOS", "Empty", 1, 106001)
    );
    assert_eq!(c.launcher.program, "emacsclient");
    assert_eq!(c.launcher.args, ["-c", "-n"]);
    assert_eq!(
        c.server_path,
        Path::new(r"C:\Programs\VcXsrvWSL\vcxsrv.exe")
    );
}
#[test]
fn rejects_invalid_fields_without_echoing_values() {
    let base = Config::defaults_for(Path::new(r"C:\App\manager.exe")).unwrap();
    let mut c = base.clone();
    c.version = 2;
    assert_eq!(c.validate(), Err(ConfigError::Schema));
    for name in ["", "\n", "Bad\u{85}", "\0", &"🦀".repeat(129)] {
        c = base.clone();
        c.distro = name.into();
        assert_eq!(c.validate(), Err(ConfigError::Distro));
    }
    c = base.clone();
    c.distro = "🦀".repeat(128);
    c.validate().unwrap();
    for name in ["", "\t", "Bad\u{7f}"] {
        c = base.clone();
        c.user = name.into();
        assert_eq!(c.validate(), Err(ConfigError::User));
    }
    c = base.clone();
    c.display = 100;
    assert_eq!(c.validate(), Err(ConfigError::Display));
    c = base.clone();
    c.server_path = "relative.exe".into();
    assert_eq!(c.validate(), Err(ConfigError::ServerPath));
    c = base.clone();
    c.launcher.program.clear();
    assert_eq!(c.validate(), Err(ConfigError::Launcher));
    c = base.clone();
    c.launcher.args.push("SECRET\0".into());
    assert_eq!(c.validate(), Err(ConfigError::Launcher));
    assert!(!format!("{:?}", c).contains("SECRET"));
}
#[test]
fn versioned_json_resolves_only_missing_default_path() {
    let exe = Path::new(r"C:\App\manager.exe");
    let c = Config::from_json(r#"{"version":1}"#, exe).unwrap();
    assert_eq!(c.server_path, Path::new(r"C:\App\vcxsrv.exe"));
    assert!(Config::from_json(r#"{"version":1,"server_path":"relative.exe"}"#, exe).is_err());
    assert!(Config::from_json(r#"{"version":9}"#, exe).is_err());
    assert!(Config::from_json(r#"{"version":1,"unknown":1}"#, exe).is_err());
}
