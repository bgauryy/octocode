//! Lock files the Octocode editor extension writes (`<home>/ide/<pid>.json`)
//! and the choice of the one whose editor has the request's file open.
//!
//! A lock is trusted only when it is a regular file (never a symlink) the
//! current user owns and no one else can read (it carries the bridge's
//! bearer token), in a directory (never a symlink) the current user owns
//! and no one else can write. A candidate is live when
//! its process runs and one of its workspace folders contains the file.
//! Several live candidates are ordered, never filtered, by what the process
//! says about itself: an ancestor pid first, then the terminal the user
//! works in, then the most specific folder. The environment alone never
//! enables the rung: with no lock there is no candidate to order.
use serde::Deserialize;
use std::path::{Path, PathBuf};

mod directory;
use directory::{Directory, directory};

/// The lock file's `version` this client understands.
const LOCK_VERSION: u32 = 1;

/// A lock file as the extension wrote it.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lock {
    version: u32,
    pub(super) pid: u32,
    pub ide: String,
    pub port: u16,
    pub token: String,
    workspace_folders: Vec<PathBuf>,
    /// Why the file is not trusted, when it is not; never on the wire.
    #[serde(skip)]
    refusal: Option<String>,
}

/// The token is a credential: nothing that formats a lock shows it.
impl std::fmt::Debug for Lock {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Lock")
            .field("version", &self.version)
            .field("pid", &self.pid)
            .field("ide", &self.ide)
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .field("workspace_folders", &self.workspace_folders)
            .field("refusal", &self.refusal)
            .finish()
    }
}

/// What the lock directory holds for one file.
pub enum Discovery {
    /// No lock directory, or no live lock whose folders contain the file:
    /// the rung does not apply, which is not a failure.
    Absent,
    /// The lock to use.
    Found(Box<Lock>),
    /// A lock that would have applied but is not trustworthy, with why: the
    /// rung failed, and its token was never read into a request.
    Refused(String),
}

/// What the process knows about the editor it runs under, to order
/// candidates with.
#[derive(Clone, Debug, Default)]
pub struct Hints {
    /// Pids of this process's ancestors, nearest first.
    pub ancestors: Vec<u32>,
    /// `TERM_PROGRAM` and `TERMINAL_EMULATOR`.
    pub(super) terminals: Vec<String>,
}

impl Hints {
    /// The hints of this process, read only when more than one candidate
    /// needs ordering ([`Self::of_process`] is the one place that reads the
    /// environment and the process table).
    pub fn of_process() -> Self {
        Self {
            ancestors: ancestors(std::process::id()),
            terminals: ["TERM_PROGRAM", "TERMINAL_EMULATOR"]
                .into_iter()
                .filter_map(|name| std::env::var(name).ok())
                .collect(),
        }
    }
}

/// Whether the lock directory exists and holds anything to judge (a
/// refusal included): the one stat a call with no editor pays, before any
/// work to resolve its workspace.
pub fn may_apply(home: &Path) -> bool {
    match directory(&home.join("ide")) {
        Directory::Absent => false,
        Directory::Refused(_) => true,
        Directory::Files(files) => !files.is_empty(),
    }
}

/// Discover the lock whose editor can answer for `file` (a canonical path).
pub(super) fn discover(home: &Path, file: &Path, hints: impl FnOnce() -> Hints) -> Discovery {
    let files = match directory(&home.join("ide")) {
        Directory::Absent => return Discovery::Absent,
        Directory::Refused(reason) => return Discovery::Refused(reason),
        Directory::Files(files) => files,
    };
    let mut live = Vec::new();
    let mut refused = None;
    for path in files {
        let lock = match read(&path) {
            // A file that is not a lock of this protocol is not ours to judge.
            Read::Foreign => continue,
            // Not opened: only its name says whose editor it claims to be.
            Read::Symlink { pid, reason } => {
                if pid_alive(pid) {
                    refused.get_or_insert(reason);
                }
                continue;
            }
            Read::Lock(lock) => lock,
        };
        if !pid_alive(lock.pid) {
            continue;
        }
        let Some(depth) = lock.folder_depth(file) else {
            continue;
        };
        match &lock.refusal {
            Some(reason) => {
                refused.get_or_insert_with(|| reason.clone());
            }
            None => live.push((depth, lock)),
        }
    }
    if live.is_empty() {
        return refused.map_or(Discovery::Absent, Discovery::Refused);
    }
    if live.len() > 1 {
        let hints = hints();
        live.sort_by_key(|(depth, lock)| lock.rank(&hints, *depth));
    }
    live.into_iter()
        .next()
        .map_or(Discovery::Absent, |(_, lock)| {
            Discovery::Found(Box::new(lock))
        })
}

impl Lock {
    /// Components of the most specific workspace folder that contains
    /// `file`; `None` when no folder does.
    fn folder_depth(&self, file: &Path) -> Option<usize> {
        self.workspace_folders
            .iter()
            .map(|folder| std::fs::canonicalize(folder).unwrap_or_else(|_| folder.clone()))
            .filter(|folder| file.starts_with(folder))
            .map(|folder| folder.components().count())
            .max()
    }

    /// Sort key: ancestor pid, terminal match, then the most specific folder.
    fn rank(&self, hints: &Hints, depth: usize) -> (usize, bool, std::cmp::Reverse<usize>) {
        let ancestor = hints
            .ancestors
            .iter()
            .position(|pid| *pid == self.pid)
            .unwrap_or(usize::MAX);
        let terminal = !hints
            .terminals
            .iter()
            .any(|terminal| names_editor(terminal, &self.ide));
        (ancestor, terminal, std::cmp::Reverse(depth))
    }
}

/// Whether a terminal program (`vscode`, `JetBrains-JediTerm`) names the
/// editor called `ide` (`Visual Studio Code`, `IntelliJ IDEA`).
fn names_editor(terminal: &str, ide: &str) -> bool {
    let squash = |text: &str| {
        text.chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_lowercase()
    };
    let (terminal, ide) = (squash(terminal), squash(ide));
    !terminal.is_empty()
        && (ide.contains(&terminal)
            || terminal.contains(&ide)
            || (terminal == "vscode" && ide.contains("code")))
}

/// What one directory entry is.
enum Read {
    /// Not a lock of this protocol.
    Foreign,
    /// A lock, trusted or (with its `refusal`) not.
    Lock(Lock),
    /// A symbolic link named like the lock of process `pid`: never opened.
    Symlink { pid: u32, reason: String },
}

/// Why the lock `path` was not used.
fn refusal(path: &Path, problem: &str) -> String {
    format!(
        "The IDE bridge lock {} {problem}; it was not used.",
        path.file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    )
}

/// The entry at `path`. A lock that is not safe to trust (see [`untrusted`])
/// keeps its `refusal` and loses its token; a symbolic link is not opened.
fn read(path: &Path) -> Read {
    use std::io::Read as _;
    let Ok(entry) = std::fs::symlink_metadata(path) else {
        return Read::Foreign;
    };
    if entry.file_type().is_symlink() {
        return path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse().ok())
            .map_or(Read::Foreign, |pid| Read::Symlink {
                pid,
                reason: refusal(path, "is a symbolic link"),
            });
    }
    if !entry.is_file() {
        return Read::Foreign;
    }
    let Ok(mut file) = open_without_following(path) else {
        return Read::Foreign;
    };
    let Some(metadata) = file.metadata().ok().filter(std::fs::Metadata::is_file) else {
        return Read::Foreign;
    };
    let mut text = String::new();
    if file.read_to_string(&mut text).is_err() {
        return Read::Foreign;
    }
    let Some(mut lock) = serde_json::from_str::<Lock>(&text)
        .ok()
        .filter(|lock| lock.version == LOCK_VERSION && !lock.token.is_empty())
    else {
        return Read::Foreign;
    };
    if let Some(problem) = untrusted(&metadata) {
        lock.token.clear();
        lock.refusal = Some(refusal(path, problem));
    }
    Read::Lock(lock)
}

/// Opens `path` unless it is a symbolic link by then (it was not one when
/// its entry was read).
#[cfg(unix)]
fn open_without_following(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[cfg(not(unix))]
fn open_without_following(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::File::open(path)
}

/// Why a lock file cannot be trusted with the bridge's token.
#[cfg(unix)]
fn untrusted(metadata: &std::fs::Metadata) -> Option<&'static str> {
    use std::os::unix::fs::MetadataExt;
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Some("is not owned by the current user");
    }
    (metadata.mode() & 0o077 != 0).then_some("is accessible to its group or others")
}

/// Windows keeps the file in the user's profile; its ACLs are not read.
#[cfg(not(unix))]
fn untrusted(_: &std::fs::Metadata) -> Option<&'static str> {
    None
}

/// Whether a process with `pid` runs.
#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    let Ok(raw) = libc::pid_t::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 only checks that the process exists and may be signalled.
    let signalled = unsafe { libc::kill(raw, 0) } == 0;
    // EPERM: the process exists but belongs to another user.
    signalled || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, STILL_ACTIVE};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: the handle is owned here and closed exactly once.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0;
        let queried = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        queried && i32::try_from(code) == Ok(STILL_ACTIVE)
    }
}

/// The parent chain of `pid`, nearest first, read from the kernel's process
/// table (no child process): empty where this platform offers no read, which
/// only loses the ancestor preference.
#[cfg(unix)]
fn ancestors(pid: u32) -> Vec<u32> {
    let mut chain = Vec::new();
    let mut next = pid;
    while let Some(parent) = octocode_engine::process::parent_pid(next)
        .filter(|parent| *parent > 1 && !chain.contains(parent))
    {
        chain.push(parent);
        next = parent;
    }
    chain
}

#[cfg(not(unix))]
fn ancestors(_: u32) -> Vec<u32> {
    Vec::new()
}

/// The editor lock that answers for `path` (canonical) in `workspace`:
/// none for code Octocode fetched (no editor is asked about code the server
/// rung would refuse to open), else the live, trusted lock whose folders hold
/// the file ([`discover`]).
pub fn editor_lock(
    discovery: &octocode_engine::lsp::config::LspDiscoveryOptions,
    home: &std::path::Path,
    path: &str,
    workspace: &str,
    hints: impl FnOnce() -> Hints,
) -> Discovery {
    if discovery.opens_fetched_code(path, workspace) {
        return Discovery::Absent;
    }
    discover(home, Path::new(path), hints)
}
