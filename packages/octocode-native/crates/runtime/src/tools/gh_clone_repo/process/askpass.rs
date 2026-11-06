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
        let directory = parent.join(format!("askpass-{}", hex::encode(nonce)));
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
