//! The browser session belongs to the npm management package.
use std::process::Command;

pub fn run(no_open: bool, idle_timeout: u64) -> u8 {
    let hint = "npx -y octocode config view";
    if std::env::var_os("OCTOCODE_CONFIG_VIEW_DELEGATED").is_some() {
        eprintln!("The npm Octocode launcher is required for config view. Run: {hint}");
        return 5;
    }
    let Some(launcher) = super::skill::find_launcher(std::env::var_os("PATH").as_deref()) else {
        eprintln!("The npm Octocode launcher is required for config view. Run: {hint}");
        return 5;
    };
    if super::skill::is_current_exe(&launcher) {
        eprintln!("The npm Octocode launcher is required for config view. Run: {hint}");
        return 5;
    }
    let mut command = Command::new(launcher);
    command
        .args([
            "config",
            "view",
            "--idle-timeout",
            &idle_timeout.to_string(),
        ])
        .env("OCTOCODE_CONFIG_VIEW_DELEGATED", "1");
    if no_open {
        command.arg("--no-open");
    }
    match command.status() {
        Ok(status) => status
            .code()
            .and_then(|code| u8::try_from(code).ok())
            .unwrap_or(5),
        Err(_) => {
            eprintln!("Cannot open the config view. Run: {hint}");
            5
        }
    }
}
