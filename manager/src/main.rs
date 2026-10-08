#![windows_subsystem = "windows"]
use vcxsrv_wsl_manager::{
    cli::{Mode, Options},
    tray, worker,
};
fn main() -> std::process::ExitCode {
    let options = match Options::parse(std::env::args_os().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            return std::process::ExitCode::from(2);
        }
    };
    let result = match options.mode {
        Mode::Help => {
            println!("VcXsrv WSL Manager\n  --tray [--state-dir ABSOLUTE_DIR] [--config ABSOLUTE_FILE]\n  doctor --json [--state-dir ABSOLUTE_DIR] [--config ABSOLUTE_FILE]\n  recover-stopped [--state-dir ABSOLUTE_DIR] [--config ABSOLUTE_FILE]\nState/config require protected current-user security. One manager per Windows user across sessions. Startup is opt-in.");
            Ok(())
        }
        Mode::Doctor => worker::doctor(&options)
            .map(|snapshot| println!("{}", serde_json::to_string(&snapshot).unwrap())),
        Mode::Recover => worker::recover_stopped(&options)
            .map(|result| println!("{result:?}; authority preserved. Start server separately.")),
        Mode::Tray => tray::run(options),
    };
    match result {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(1)
        }
    }
}
