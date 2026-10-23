//! The per-run GIT_ASKPASS credential helper: answers only the authorized origin's prompts.
use super::CloneError;
use octocode_contract_types::ErrorCode;
use std::path::{Path, PathBuf};

/// An owner-only GIT_ASKPASS helper for one git run, removed on drop. Git
/// asks it only after the server challenges; it answers with the
/// `x-access-token` user and the token for the authorized origin, and with
/// nothing (a failure) for any other prompt, so a redirect to another host
/// never receives the credential.
pub(super) struct AskPass {
    directory: PathBuf,
    pub(super) script: PathBuf,
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
        };
        let body = format!(
            "#!/bin/sh\ncase \"$1\" in\n\
             {user}*) printf '%s\\n' x-access-token ;;\n\
             {password}*) printf '%s\\n' {token} ;;\n\
             *) exit 1 ;;\n\
             esac\n",
            user = shell_quote(&format!("Username for '{scheme}://{authority}'")),
            password = shell_quote(&format!(
                "Password for '{scheme}://x-access-token@{authority}'"
            )),
            token = shell_quote(token),
        );
        let mut file = octocode_io::atomic::create_new(&helper.script, 0o700)
            .map_err(|error| failed(error.to_string()))?;
        std::io::Write::write_all(&mut file, body.as_bytes())
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
}

impl Drop for AskPass {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// `text` as one single-quoted POSIX shell word.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}
