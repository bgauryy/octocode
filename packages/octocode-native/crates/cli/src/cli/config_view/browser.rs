//! The config view's window: a Chromium-family browser in kiosk mode on a
//! throwaway profile, so nothing else in the browser can read the secrets typed
//! into the page. Extensions, sync, saved passwords and other sites have no way
//! in, the browser cannot reach anything but this computer, and the profile is
//! deleted when the session ends.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use tokio::process::{Child, Command};

/// The only variables the browser inherits. Everything else, tokens included,
/// stays out of its environment.
const KEPT_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "TEMP",
    "TMP",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XAUTHORITY",
    "XDG_RUNTIME_DIR",
    "DBUS_SESSION_BUS_ADDRESS",
    "SystemRoot",
    "SystemDrive",
    "USERPROFILE",
    "LOCALAPPDATA",
    "APPDATA",
    "ProgramFiles",
    "ProgramFiles(x86)",
];

#[cfg(target_os = "macos")]
const APPLICATIONS: &[&str] = &[
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
    "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
];
#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
const NAMES: &[&str] = &[
    "google-chrome",
    "google-chrome-stable",
    "microsoft-edge",
    "brave-browser",
    "chromium",
    "chromium-browser",
];
#[cfg(target_os = "windows")]
const WINDOWS_SUFFIXES: &[&str] = &[
    r"Google\Chrome\Application\chrome.exe",
    r"Microsoft\Edge\Application\msedge.exe",
    r"BraveSoftware\Brave-Browser\Application\brave.exe",
];

/// The first entry of a `PATH`-style list holding an executable `name`.
#[cfg(any(test, all(not(target_os = "windows"), not(target_os = "macos"))))]
fn on_path(name: &str, path: &std::ffi::OsStr) -> Option<PathBuf> {
    std::env::split_paths(path)
        .filter(|directory| directory.is_absolute())
        .map(|directory| directory.join(name))
        .find(|candidate| candidate.is_file())
}

/// Browsers to try, by absolute path only: a relative `PATH` entry could be a
/// checkout's own executable.
fn candidates() -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    return APPLICATIONS.iter().map(PathBuf::from).collect();
    #[cfg(target_os = "windows")]
    return ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"]
        .iter()
        .filter_map(std::env::var_os)
        .flat_map(|root| {
            WINDOWS_SUFFIXES
                .iter()
                .map(move |suffix| PathBuf::from(&root).join(suffix))
        })
        .collect();
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    return std::env::var_os("PATH")
        .map(|path| {
            NAMES
                .iter()
                .filter_map(|name| on_path(name, &path))
                .collect()
        })
        .unwrap_or_default();
}

/// The browser's arguments. Chromium opens the last one. Nothing here may need a
/// script, a debugging port or a network: the page is self-contained.
fn arguments(page: &str, profile: &Path) -> Vec<OsString> {
    let mut profile_flag = OsString::from("--user-data-dir=");
    profile_flag.push(profile);
    let mut arguments = vec![profile_flag];
    arguments.extend(
        [
            "--kiosk",
            // Nothing but this computer resolves, so no script, font or beacon can
            // load even if a page asked for one.
            "--host-resolver-rules=MAP * ~NOTFOUND , EXCLUDE 127.0.0.1",
            "--disable-extensions",
            "--disable-component-extensions-with-background-pages",
            "--disable-default-apps",
            "--disable-sync",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-domain-reliability",
            "--disable-client-side-phishing-detection",
            "--disable-breakpad",
            "--disable-notifications",
            "--deny-permission-prompts",
            "--disable-features=Translate,OptimizationHints,MediaRouter,AutofillServerCommunication",
            "--no-first-run",
            "--no-default-browser-check",
            "--no-pings",
            "--password-store=basic",
            "--use-mock-keychain",
        ]
        .map(OsString::from),
    );
    arguments.push(OsString::from(page));
    arguments
}

/// The session link, in a file only this user can read: a command line is visible
/// to every user on the machine, and this link holds the one-use credential.
fn launch_page(profile: &Path, url: &str) -> Option<String> {
    let path = profile.join("open-octocode.html");
    let page = format!(
        "<!doctype html><meta charset=utf-8><meta http-equiv=refresh content=\"0;url={url}\">"
    );
    std::fs::write(&path, page).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).ok()?;
    }
    url::Url::from_file_path(&path).ok().map(String::from)
}

/// A kiosk window on its own profile, closed and deleted when the session ends.
pub(super) struct IsolatedWindow {
    child: Child,
    profile: tempfile::TempDir,
}

impl IsolatedWindow {
    /// The first browser that starts, or `None` when no supported one does.
    pub(super) fn open(url: &str) -> Option<Self> {
        Self::open_first(candidates(), url)
    }

    fn open_first(browsers: Vec<PathBuf>, url: &str) -> Option<Self> {
        browsers.into_iter().find_map(|browser| {
            let mut builder = tempfile::Builder::new();
            builder.prefix("octocode-config-view-");
            // A temp directory otherwise takes the umask, and this one holds the session link.
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                builder.permissions(std::fs::Permissions::from_mode(0o700));
            }
            let profile = builder.tempdir().ok()?;
            let page = launch_page(profile.path(), url)?;
            let mut command = Command::new(&browser);
            command
                .args(arguments(&page, profile.path()))
                .env_clear()
                .envs(
                    KEPT_ENV
                        .iter()
                        .filter_map(|name| std::env::var_os(name).map(|value| (name, value))),
                )
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);
            let child = command.spawn().ok()?;
            Some(Self { child, profile })
        })
    }

    pub(super) async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait().await
    }

    /// Close the window, then delete everything the browser wrote.
    pub(super) async fn close(mut self) {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        let Self { profile, .. } = self;
        let _ = tokio::task::spawn_blocking(move || profile.close()).await;
    }
}

/// Whether this terminal's user can see a window opened from here. A remote or
/// display-less session cannot, so `octocode config` prints instead.
pub(in crate::cli) fn can_show_window(env: impl Fn(&str) -> Option<OsString>) -> bool {
    let set = |name: &str| env(name).is_some_and(|value: OsString| !value.is_empty());
    if set("SSH_CONNECTION") || set("SSH_TTY") {
        return false;
    }
    cfg!(any(target_os = "macos", target_os = "windows"))
        || set("DISPLAY")
        || set("WAYLAND_DISPLAY")
}

#[cfg(test)]
mod tests {
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
        std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o755))
            .expect("executable");

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
            IsolatedWindow::open_first(vec![PathBuf::from("/nonexistent/browser")], "url")
                .is_none()
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
}
