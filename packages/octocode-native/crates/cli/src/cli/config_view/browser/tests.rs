use super::*;

fn text(arguments: &[OsString]) -> Vec<String> {
    arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_window_is_a_kiosk_on_its_own_profile_with_extensions_off() {
    let profile = Path::new("/synthetic/profile");
    let page = "file:///synthetic/profile/open-octocode.html";
    let arguments = text(&arguments(page, profile));
    for required in [
        "--kiosk",
        "--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE 127.0.0.1",
        "--disable-extensions",
        "--disable-sync",
        "--no-first-run",
        "--password-store=basic",
        "--user-data-dir=/synthetic/profile",
    ] {
        assert!(arguments.iter().any(|a| a == required), "{required}");
    }
    assert_eq!(arguments.last().map(String::as_str), Some(page));
    for forbidden in [
        "remote-debugging",
        "disable-web-security",
        "allow-file-access",
        "load-extension",
        "no-sandbox",
        "unsafely",
    ] {
        assert!(
            !arguments.iter().any(|a| a.contains(forbidden)),
            "{forbidden}"
        );
    }
}

#[test]
fn the_browser_never_inherits_a_credential() {
    for name in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "OCTOCODE_CLASSIFICATION_API",
        "OCTOCODE_HOME",
        "NODE_OPTIONS",
    ] {
        assert!(!KEPT_ENV.contains(&name), "{name}");
    }
}

#[test]
fn a_relative_path_entry_is_never_a_browser() {
    let directory = tempfile::tempdir().expect("directory");
    let browser = directory.path().join("chromium");
    std::fs::write(&browser, "").expect("browser");
    let absolute = std::env::join_paths([directory.path()]).expect("path");
    assert_eq!(on_path("chromium", &absolute), Some(browser));
    assert_eq!(
        on_path("chromium", std::ffi::OsStr::new("relative/dir")),
        None
    );
    assert_eq!(on_path("absent", &absolute), None);
}

/// A stand-in browser records what it was given, then stays open like a window.
#[cfg(unix)]
#[tokio::test]
async fn the_window_runs_clean_and_leaves_nothing_behind_when_closed() {
    use std::os::unix::fs::PermissionsExt as _;
    let directory = tempfile::tempdir().expect("directory");
    let record = directory.path().join("record");
    let browser = directory.path().join("browser");
    std::fs::write(
        &browser,
        format!(
            "#!/bin/sh\n{{ printf '%s\\n' \"$@\"; echo --env--; env; }} > '{}.tmp'\nmv '{0}.tmp' '{0}'\nexec sleep 60\n",
            record.display()
        ),
    )
    .expect("write browser");
    std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o755)).expect("executable");

    let missing = directory.path().join("absent");
    let url = "http://127.0.0.1:1/#one-use-credential";
    let window = IsolatedWindow::open_first(vec![missing, browser], url)
        .expect("the next browser starts when one is missing");
    let profile = window.profile.path().to_owned();
    assert!(profile.is_dir());

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !record.exists() {
        assert!(std::time::Instant::now() < deadline, "browser did not run");
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let recorded = std::fs::read_to_string(&record).expect("record");
    let (arguments, environment) = recorded.split_once("--env--").expect("sections");
    assert!(
        arguments.lines().any(|line| line == "--kiosk"),
        "{arguments}"
    );
    // The credential is not on the command line, where any user could read it.
    assert!(!arguments.contains("one-use-credential"), "{arguments}");
    let opened = arguments.lines().last().expect("page argument");
    let launch = url::Url::parse(opened)
        .expect("file URL")
        .to_file_path()
        .expect("path");
    assert!(launch.starts_with(&profile), "{opened}");
    assert!(
        std::fs::read_to_string(&launch)
            .expect("page")
            .contains(url)
    );
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&launch)
            .expect("page")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "only this user may read the link");
        let parent = std::fs::metadata(&profile)
            .expect("profile")
            .permissions()
            .mode();
        assert_eq!(parent & 0o077, 0, "the profile is private");
    }
    // Cargo's variables prove the environment was cleared, not inherited.
    assert!(!environment.contains("CARGO_"), "{environment}");
    assert!(!environment.contains("RUST_"), "{environment}");

    window.close().await;
    assert!(!profile.exists(), "the profile outlived the session");
}

#[test]
fn no_supported_browser_means_no_window() {
    assert!(
        IsolatedWindow::open_first(vec![PathBuf::from("/nonexistent/browser")], "url").is_none()
    );
}

#[test]
fn candidates_are_absolute_paths() {
    assert!(candidates().iter().all(|path| path.is_absolute()));
}

#[test]
fn a_remote_or_display_less_session_prints_instead_of_opening() {
    let session = |vars: &'static [(&'static str, &'static str)]| {
        can_show_window(move |name: &str| {
            vars.iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        })
    };
    assert!(!session(&[("SSH_CONNECTION", "1.2.3.4 1 5.6.7.8 22")]));
    assert!(!session(&[("SSH_TTY", "/dev/ttys001"), ("DISPLAY", ":0")]));
    assert!(session(&[("DISPLAY", ":0")]));
    assert!(session(&[("WAYLAND_DISPLAY", "wayland-0")]));
    assert_eq!(
        session(&[]),
        cfg!(any(target_os = "macos", target_os = "windows"))
    );
}
