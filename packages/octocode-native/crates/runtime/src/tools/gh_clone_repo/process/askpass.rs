//! The per-run GIT_ASKPASS credential helper: answers only the authorized origin's prompts.
use super::CloneError;
use octocode_contract_types::ErrorCode;
use std::path::{Path, PathBuf};

/// The child environment names the helper reads: the two prompts it may
/// answer and the token it answers the password prompt with.
const USERNAME_PROMPT: &str = "OCTOCODE_ASKPASS_USERNAME_PROMPT";
const PASSWORD_PROMPT: &str = "OCTOCODE_ASKPASS_PASSWORD_PROMPT";
const TOKEN: &str = "OCTOCODE_ASKPASS_TOKEN";

/// The helper git runs: a fixed script that holds no credential. It answers
/// a prompt starting with the username prompt with the `x-access-token`
/// user, one starting with the password prompt with the token, and fails on
/// anything else, or when a prompt or the token is unset or empty (an empty
/// pattern would match every prompt). Quoted expansions in a `case` pattern
/// match literally, and only shell builtins run, so the token never reaches
/// another process's argv and the helper needs no PATH.
const SCRIPT: &str = concat!(
    "#!/bin/sh\n",
    "case \"$OCTOCODE_ASKPASS_USERNAME_PROMPT\" in '') exit 1 ;; esac\n",
    "case \"$OCTOCODE_ASKPASS_PASSWORD_PROMPT\" in '') exit 1 ;; esac\n",
    "case \"$OCTOCODE_ASKPASS_TOKEN\" in '') exit 1 ;; esac\n",
    "case \"$1\" in\n",
    "\"$OCTOCODE_ASKPASS_USERNAME_PROMPT\"*) printf '%s\\n' x-access-token ;;\n",
    "\"$OCTOCODE_ASKPASS_PASSWORD_PROMPT\"*) printf '%s\\n' \"$OCTOCODE_ASKPASS_TOKEN\" ;;\n",
    "*) exit 1 ;;\n",
    "esac\n",
);

const DIRECTORY_PREFIX: &str = "askpass-";

/// The pid a helper directory name (`askpass-<pid>-<nonce>`) records, when
/// it is one this tool created.
fn owner_pid(name: &str) -> Option<u32> {
    let (pid, nonce) = name.strip_prefix(DIRECTORY_PREFIX)?.split_once('-')?;
    let hex_nonce = nonce.len() == 32 && nonce.bytes().all(|byte| byte.is_ascii_hexdigit());
    (hex_nonce && !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| pid.parse().ok())
        .flatten()
}

/// Whether a process with this pid exists. A pid that is reused by another
/// process reads as alive, which only keeps a stale directory longer.
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return true;
    };
    // SAFETY: signal 0 only checks that the pid can be signalled.
    let reached = unsafe { libc::kill(pid, 0) } == 0;
    reached || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Without a process probe a directory is never judged dead by pid.
#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    true
}

/// Removes the helper directories a killed git run left behind: only real
/// directories named like this tool's, whose recorded owner process is gone
/// (a live run, in this process or another, is never touched). Directories
/// whose name records no pid are left alone. Best effort: a
/// directory that cannot be removed stays for the next run.
fn sweep_stale(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(metadata) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !metadata.is_dir() {
            continue;
        }
        let stale =
            owner_pid(&name).is_some_and(|pid| pid != std::process::id() && !process_alive(pid));
        if stale {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

/// An owner-only GIT_ASKPASS helper for one git run, removed on drop. Git
/// asks it only after the server challenges; it answers with the
/// `x-access-token` user and the token for the authorized origin, and with
/// nothing (a failure) for any other prompt, so a redirect to another host
/// never receives the credential. The token is never written to disk: it
/// lives in [`AskPass::env`], set on this one git child, so a killed run
/// leaves no credential behind.
pub(super) struct AskPass {
    directory: PathBuf,
    pub(super) script: PathBuf,
    env: [(&'static str, String); 3],
}

impl AskPass {
    pub(super) fn create(parent: &Path, token: &str, url: &str) -> Result<Self, CloneError> {
        let failed = |reason: String| {
            CloneError::new(
                ErrorCode::GitFailed,
                format!("Failed to create the Git credential helper: {reason}"),
            )
        };
        let url = url::Url::parse(url).map_err(|error| failed(error.to_string()))?;
        let host = url
            .host_str()
            .ok_or_else(|| failed("the authorized URL names no host".into()))?;
        let authority = match url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_owned(),
        };
        let scheme = url.scheme();
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(|error| failed(error.to_string()))?;
        sweep_stale(parent);
        let directory = parent.join(format!(
            "{DIRECTORY_PREFIX}{}-{}",
            std::process::id(),
            hex::encode(nonce)
        ));
        let builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            let mut builder = builder;
            std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
            builder
        };
        builder
            .create(&directory)
            .map_err(|error| failed(error.to_string()))?;
        let helper = Self {
            script: directory.join("askpass"),
            directory,
            env: [
                (
                    USERNAME_PROMPT,
                    format!("Username for '{scheme}://{authority}'"),
                ),
                (
                    PASSWORD_PROMPT,
                    format!("Password for '{scheme}://x-access-token@{authority}'"),
                ),
                (TOKEN, token.to_owned()),
            ],
        };
        let mut file = octocode_io::atomic::create_new(&helper.script, 0o700)
            .map_err(|error| failed(error.to_string()))?;
        std::io::Write::write_all(&mut file, SCRIPT.as_bytes())
            .map_err(|error| failed(error.to_string()))?;
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&helper.script, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| failed(error.to_string()))?;
        }
        Ok(helper)
    }

    /// The environment the helper reads, for the git child alone: the
    /// authorized origin's prompts and the token.
    pub(super) fn env(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.env.iter().map(|(name, value)| (*name, value.as_str()))
    }
}

impl Drop for AskPass {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
