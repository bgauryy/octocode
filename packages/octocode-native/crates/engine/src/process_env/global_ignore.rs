//! The global ignore file of one call: the file git's `core.excludesFile`
//! names, resolved from the environment the call carries, never from the
//! process. A host that keeps one process for many calls hands each call its
//! own environment, so two calls with different `GIT_CONFIG_GLOBAL` or
//! `XDG_CONFIG_HOME` see different global rules.
//!
//! The precedence is the `ignore` crate's own: the file `GIT_CONFIG_GLOBAL`
//! names, `$HOME/.gitconfig`, `$XDG_CONFIG_HOME/git/config` (`~/.config`
//! without it), the system config (`GIT_CONFIG_SYSTEM`, else
//! `/etc/gitconfig`), each the first to set `core.excludesFile`; with none,
//! `$XDG_CONFIG_HOME/git/ignore`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use super::{EnvMap, call_var};

/// What one environment resolves to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Resolved {
    /// The global ignore file, which may not exist.
    file: Option<PathBuf>,
    /// Every file whose contents decide the rules: the file and the git
    /// config files that may name it.
    inputs: Vec<PathBuf>,
}

/// The global ignore rules a call walks under. `Copy`: resolved values are
/// kept once per distinct result for the life of the process (a handful: one
/// per distinct file), so a walk policy carries it by reference.
///
/// The default has no global rules; a call's rules come from
/// [`GlobalIgnore::resolve`] over the environment it carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GlobalIgnore(Option<&'static Resolved>);

impl GlobalIgnore {
    /// No global rules.
    pub const NONE: Self = Self(None);
}

static KEPT: Mutex<Vec<&'static Resolved>> = Mutex::new(Vec::new());

impl GlobalIgnore {
    /// The rules `env` (a call's environment) names; `home` is the home
    /// used when `env` has no `HOME`.
    pub fn resolve(env: &impl EnvMap, home: Option<&Path>) -> Self {
        let resolved = resolve_with(&|name| call_var(env, name), home.map(Path::to_path_buf));
        let mut kept = KEPT.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(known) = kept.iter().find(|known| ***known == resolved) {
            return Self(Some(known));
        }
        let leaked: &'static Resolved = Box::leak(Box::new(resolved));
        kept.push(leaked);
        Self(Some(leaked))
    }

    /// The global ignore file; it may not exist.
    pub fn file(self) -> Option<PathBuf> {
        self.0.and_then(|resolved| resolved.file.clone())
    }

    /// The files whose contents decide the global rules: a listed path may
    /// not exist, and creating it changes the rules.
    pub fn inputs(self) -> Vec<PathBuf> {
        self.0
            .map_or_else(Vec::new, |resolved| resolved.inputs.clone())
    }
}

fn resolve_with(
    read: &dyn Fn(&'static str) -> Option<OsString>,
    fallback_home: Option<PathBuf>,
) -> Resolved {
    let set = |name: &'static str| {
        read(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    let home = set("HOME").or(fallback_home);
    let xdg = set("XDG_CONFIG_HOME").or_else(|| home.as_ref().map(|home| home.join(".config")));
    let system = set("GIT_CONFIG_SYSTEM").unwrap_or_else(|| PathBuf::from("/etc/gitconfig"));
    let configs: Vec<PathBuf> = set("GIT_CONFIG_GLOBAL")
        .into_iter()
        .chain(home.as_ref().map(|home| home.join(".gitconfig")))
        .chain(xdg.as_ref().map(|dir| dir.join("git/config")))
        .chain(Some(system))
        .collect();
    let named = configs
        .iter()
        .find_map(|config| excludes_file(&std::fs::read(config).ok()?, home.as_deref()));
    let file = named.or_else(|| xdg.map(|dir| dir.join("git/ignore")));
    let mut inputs: Vec<PathBuf> = file.iter().cloned().chain(configs).collect();
    let mut seen = std::collections::HashSet::new();
    inputs.retain(|path| seen.insert(path.clone()));
    Resolved { file, inputs }
}

/// `core.excludesFile` in a git config's `data`, read as the `ignore` crate
/// reads it: the first `excludesfile = value` line (key in any case, one
/// optional pair of quotes, no inner whitespace), `~` standing for `home`.
fn excludes_file(data: &[u8], home: Option<&Path>) -> Option<PathBuf> {
    for line in String::from_utf8_lossy(data).lines() {
        let line = line.trim_start();
        let Some(key) = line.get(.."excludesfile".len()) else {
            continue;
        };
        if !key.eq_ignore_ascii_case("excludesfile") {
            continue;
        }
        let Some(value) = line["excludesfile".len()..].trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim();
        let value = value.strip_prefix('"').unwrap_or(value).trim_start();
        let value = value.strip_suffix('"').unwrap_or(value).trim_end();
        if value.is_empty() || value.contains(char::is_whitespace) {
            continue;
        }
        return Some(PathBuf::from(match home {
            Some(home) => value.replace('~', &home.to_string_lossy()),
            None => value.to_owned(),
        }));
    }
    None
}
