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
mod tests;
