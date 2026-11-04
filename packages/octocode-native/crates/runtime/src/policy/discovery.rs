use std::borrow::Cow;
use std::path::Path;

mod admission;
#[cfg(test)]
mod tests;
mod withheld;

pub(crate) use admission::{WalkAdmission, WalkRules, WalkSkips};
pub use withheld::Withheld;

pub(crate) const SENSITIVE_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    ".ssh",
    ".aws",
    ".docker",
    ".azure",
    ".kube",
    ".terraform",
    ".password-store",
    ".thunderbird",
    ".evolution",
    ".vagrant",
    ".minikube",
    ".bitcoin",
    ".ethereum",
    ".electrum",
    ".gnupg",
];

/// Credential-shaped file names no local tool reads, lists or searches,
/// whatever its flags: the one list behind [`is_sensitive_path`] (read and
/// tree) and the text search's path filter. `.env` and `.env.*` are matched
/// by prefix beside it. Lowercase, as every name is compared [`folded`].
pub(crate) const CREDENTIAL_FILE_NAMES: &[&str] = &[
    ".npmrc",
    ".pypirc",
    ".netrc",
    ".pgpass",
    ".htpasswd",
    ".vault-token",
    ".s3cfg",
    ".boto",
    ".my.cnf",
    ".dockercfg",
    ".envrc",
    ".flaskenv",
    ".git-credentials",
    ".bash_history",
    ".zsh_history",
    "credentials",
    ".credentials",
    "known_hosts",
    "authorized_keys",
    "kubeconfig",
    "terraform.tfstate",
    "terraform.tfvars",
    "master.key",
    "shadow",
    "gshadow",
    "wallet.dat",
    ".secrets",
    ".secret",
    "secrets.json",
    "secrets.yaml",
    "secrets.yml",
    "credentials.json",
    "credentials.yaml",
    "credentials.yml",
    "auth.json",
    "auth.yaml",
    "auth.yml",
    "api-keys.json",
    "api_keys.json",
    "service-account.json",
    "service_account.json",
    "private-key.pem",
    "private_key.pem",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "keyfile",
    "keyfile.json",
    "gcloud-service-key.json",
    "firebase-adminsdk.json",
    "google-services.json",
    "googleservice-info.plist",
];

/// Credential-shaped file extensions, shared like [`CREDENTIAL_FILE_NAMES`].
/// `.env` covers `app.env`/`config.env`; a backup suffix (`id_rsa.old`) is
/// matched by [`is_sensitive_path`] on the stem.
pub(crate) const CREDENTIAL_FILE_EXTENSIONS: &[&str] = &[
    ".key",
    ".pem",
    ".p12",
    ".pfx",
    ".crt",
    ".cer",
    ".der",
    ".csr",
    ".jks",
    ".keystore",
    ".ppk",
    ".kdbx",
    ".gpg",
    ".asc",
    ".ovpn",
    ".env",
];

/// Backup/copy suffixes that keep a credential file a credential file
/// (`id_rsa.old`, `credentials.json.bak`).
const BACKUP_SUFFIXES: &[&str] = &[".old", ".bak", ".backup", ".orig", ".save", "~"];

/// OS-metadata and database file names a text search skips by default;
/// `defaultExcludes:false` searches them. Lockfiles are not here: the
/// dependency versions they pin are evidence.
pub(crate) const GENERATED_FILE_NAMES: &[&str] = &[".DS_Store", "Thumbs.db", "db.sqlite3"];

/// Generated, minified, archive, and object-file extensions a text search
/// skips by default; `defaultExcludes:false` searches them. Source maps are
/// named by their compound suffix, so a keymap or linker `.map` source stays
/// searchable. Lockfiles (`*.lock`) and patches (`*.patch`, `*.diff`) are
/// not here: they are written or reviewed by hand and hold evidence.
pub(crate) const GENERATED_FILE_EXTENSIONS: &[&str] = &[
    ".tmp", ".temp", ".cache", ".bak", ".backup", ".orig", ".swp", ".swo", ".rej", ".pid", ".exe",
    ".dll", ".so", ".dylib", ".a", ".lib", ".o", ".obj", ".bin", ".class", ".pdb", ".pyc", ".pyo",
    ".pyd", ".jar", ".war", ".db", ".sqlite", ".sqlite3", ".zip", ".tar", ".gz", ".bz2", ".xz",
    ".rar", ".7z", ".js.map", ".mjs.map", ".cjs.map", ".css.map", ".ts.map", ".min.js", ".min.css",
];

/// The default-exclude pattern a file name matches (`.DS_Store`,
/// `*.min.js`), for a count by pattern.
pub(crate) fn generated_file_pattern(name: &str) -> Option<String> {
    if let Some(exact) = GENERATED_FILE_NAMES.iter().find(|exact| **exact == name) {
        return Some((*exact).to_owned());
    }
    // The longest suffix names it: `x.min.js` is `*.min.js`, not `*.js`.
    GENERATED_FILE_EXTENSIONS
        .iter()
        .filter(|extension| name.len() > extension.len() && name.ends_with(*extension))
        .max_by_key(|extension| extension.len())
        .map(|extension| format!("*{extension}"))
}

/// Credential-store locations matched by a path fragment rather than one
/// directory name: the [`folded`] fragment, and how a notice names it.
const SENSITIVE_FRAGMENTS: &[(&str, &str)] = &[
    ("/.config/gcloud/", ".config/gcloud/"),
    ("/.config/gh/", ".config/gh/"),
    ("/.config/hub/", ".config/hub/"),
    ("/.mozilla/firefox/", ".mozilla/firefox/"),
    ("/library/keychains/", "Library/Keychains/"),
];

/// `text` as a case-insensitive filesystem compares it. APFS and NTFS open
/// `.SSH` for `.ssh`, so that spelling is the withheld directory; APFS also
/// folds these non-ASCII letters to ASCII (each one measured on APFS). A volume
/// is not asked which it does for every path walked, so the safe side holds
/// everywhere: on a case-sensitive filesystem `.SSH` or `KEY.PEM` is withheld too.
fn folded(text: &str) -> Cow<'_, str> {
    if !text
        .bytes()
        .any(|byte| byte.is_ascii_uppercase() || !byte.is_ascii())
    {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for letter in text.chars() {
        match letter {
            'ß' | 'ẞ' => out.push_str("ss"),
            'ſ' => out.push('s'),
            '\u{212A}' => out.push('k'),
            'ﬀ' => out.push_str("ff"),
            'ﬁ' => out.push_str("fi"),
            'ﬂ' => out.push_str("fl"),
            'ﬃ' => out.push_str("ffi"),
            'ﬄ' => out.push_str("ffl"),
            'ﬅ' | 'ﬆ' => out.push_str("st"),
            other => out.push(other.to_ascii_lowercase()),
        }
    }
    Cow::Owned(out)
}

/// What a path is, as its caller knows it, for [`is_sensitive_path`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    Dir,
    File,
    /// Not on disk (yet), unreadable, or not looked at: file rules apply.
    Unknown,
}

impl EntryKind {
    /// The kind on disk, following symlinks; `Unknown` when it cannot be read.
    pub(crate) fn stat(path: &Path) -> Self {
        match std::fs::metadata(path) {
            Ok(meta) if meta.is_dir() => Self::Dir,
            Ok(_) => Self::File,
            Err(_) => Self::Unknown,
        }
    }
}

#[cfg(test)]
thread_local! {
    static FULL_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whole paths this thread checked ([`is_sensitive_path`]).
#[cfg(test)]
pub(crate) fn full_checks() -> usize {
    FULL_CHECKS.with(std::cell::Cell::get)
}

/// Whether the policy withholds `path`, a `kind` entry. Only a known
/// directory bypasses file-name rules (a `credentials/` folder is source);
/// unknown paths and prospective output files stay withheld, and a sensitive
/// ancestor always wins.
pub(crate) fn is_sensitive_path(path: &Path, kind: EntryKind) -> bool {
    #[cfg(test)]
    FULL_CHECKS.with(|checks| checks.set(checks.get() + 1));
    let aliases = octocode_engine::text::slash_path(path);
    sensitive_directory(&aliases).is_some()
        || (kind != EntryKind::Dir && is_sensitive_file(&aliases))
}

/// [`is_sensitive_path`] of an entry named `name` under a directory the check
/// already cleared: only the name can add a reason. `None` when the whole path
/// must tell: a name that can complete a credential location
/// ([`SENSITIVE_FRAGMENTS`]), or one that is not text.
pub(crate) fn is_sensitive_under_cleared(name: &std::ffi::OsStr, kind: EntryKind) -> Option<bool> {
    let name = folded(name.to_str()?);
    let completes = SENSITIVE_FRAGMENTS.iter().any(|(fragment, _)| {
        fragment.trim_end_matches('/').rsplit('/').next() == Some(name.as_ref())
    });
    if completes {
        return None;
    }
    Some(
        SENSITIVE_DIRECTORY_NAMES.contains(&name.as_ref())
            || (kind != EntryKind::Dir && is_sensitive_file(&name)),
    )
}

/// The policy directory a path lies in or is (`.ssh`, `.aws`,
/// `.config/gh/`), for disclosure without the path itself.
fn sensitive_directory(aliases: &str) -> Option<&'static str> {
    let aliases = folded(aliases);
    aliases
        .split('/')
        .find_map(|part| SENSITIVE_DIRECTORY_NAMES.iter().find(|name| **name == part))
        .copied()
        .or_else(|| {
            let directory = format!("{aliases}/");
            SENSITIVE_FRAGMENTS
                .iter()
                .find(|(fragment, _)| directory.contains(fragment))
                .map(|(_, label)| *label)
        })
}

/// Recovery for a path the security policy withholds: no flag or setting
/// lifts it, so a retry or a respelling cannot succeed.
pub(crate) const WITHHELD_HINT: &str = "Do not retry or respell: the security path policy withholds this path; no flag or config lifts it.";

/// The error message for a requested path the security policy withholds:
/// what it matched, and that nothing lifts it.
pub(crate) fn withheld_message(display: &str, path: &Path) -> String {
    let aliases = octocode_engine::text::slash_path(path);
    let matched = match sensitive_directory(&aliases) {
        Some(".git") if is_git_file(path, &aliases) => {
            "a `.git` file, the pointer a submodule or worktree keeps to its repository".to_owned()
        }
        Some(dir) => format!("a `{}/` directory", dir.trim_end_matches('/')),
        None => "a credential file name".to_owned(),
    };
    format!(
        "Path '{display}' is withheld by the security path policy ({matched}): no local tool reads it, and no flag or config setting lifts it."
    )
}

/// A `.git` entry that is a file (a submodule's or linked worktree's
/// `gitdir:` pointer), not the repository directory.
fn is_git_file(path: &Path, aliases: &str) -> bool {
    aliases.rsplit('/').next().map(folded).as_deref() == Some(".git") && path.is_file()
}

fn is_sensitive_file(path: &str) -> bool {
    let name = folded(path.rsplit('/').next().unwrap_or_default());
    let name = name.as_ref();
    is_credential_name(name)
        || BACKUP_SUFFIXES.iter().any(|suffix| {
            name.strip_suffix(suffix)
                .is_some_and(|stem| !stem.is_empty() && is_credential_name(stem))
        })
}

fn is_credential_name(name: &str) -> bool {
    name == ".env"
        || name.starts_with(".env.")
        || CREDENTIAL_FILE_NAMES.contains(&name)
        || CREDENTIAL_FILE_EXTENSIONS
            .iter()
            .any(|extension| name.len() > extension.len() && name.ends_with(extension))
}
