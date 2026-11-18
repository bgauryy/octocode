//! The shared store's configuration, written as files in one step.
//!
//! `git init --bare` writes a `[core]` section; the store needs more (see
//! [`render`]), and setting each key through `git config` costs one git
//! process apiece. The result is the same file `git config` would leave.

/// The shared and per-worktree config of a store: `shared` is the store's
/// `config`, `worktree` its `config.worktree`.
pub(super) struct StoreConfig {
    pub(super) shared: String,
    pub(super) worktree: String,
}

/// `init_config` is the `config` that `git init --bare` wrote: its `[core]`
/// keys (file mode, case folding, ...) describe the file system and stay.
/// `remote_url` is the `origin` the row cloned from.
///
/// Every checkout reads repository symlinks as plain files; garbage
/// collection and maintenance stay off, as only worktrees keep the commits
/// reachable and a fetch is never followed by a ref. With per-worktree
/// config, `core.bare=true` in the shared config would make every linked
/// worktree read as bare: the store alone says so, in `config.worktree`.
pub(super) fn render(init_config: &str, remote_url: &str) -> StoreConfig {
    let mut shared = String::from("[core]\n\trepositoryformatversion = 1\n");
    let mut in_core = false;
    for line in init_config.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_core = line.eq_ignore_ascii_case("[core]");
        } else if in_core && let Some((key, _)) = line.split_once('=') {
            let key = key.trim();
            if !MANAGED_CORE_KEYS
                .iter()
                .any(|managed| key.eq_ignore_ascii_case(managed))
            {
                shared.push('\t');
                shared.push_str(line);
                shared.push('\n');
            }
        }
    }
    shared.push_str("\tsymlinks = false\n");
    shared.push_str("[extensions]\n\tworktreeConfig = true\n");
    shared.push_str("[gc]\n\tauto = 0\n");
    shared.push_str("[maintenance]\n\tauto = false\n");
    shared.push_str("[remote \"origin\"]\n\turl = ");
    shared.push_str(&quote(remote_url));
    shared.push('\n');
    StoreConfig {
        shared,
        worktree: "[core]\n\tbare = true\n".into(),
    }
}

/// `[core]` keys [`render`] sets itself.
const MANAGED_CORE_KEYS: [&str; 3] = ["repositoryformatversion", "bare", "symlinks"];

/// A config value as `git config` writes it: quoted, with the characters
/// the format escapes.
fn quote(value: &str) -> String {
    let mut quoted = String::from('"');
    for character in value.chars() {
        match character {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}
