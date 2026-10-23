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
/// by prefix beside it.
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
    "GoogleService-Info.plist",
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
/// directory name.
const SENSITIVE_FRAGMENTS: &[(&str, &str)] = &[
    ("/.config/gcloud/", ".config/gcloud/"),
    ("/.config/gh/", ".config/gh/"),
    ("/.config/hub/", ".config/hub/"),
    ("/.mozilla/firefox/", ".mozilla/firefox/"),
    ("/Library/Keychains/", "Library/Keychains/"),
];

pub(crate) fn is_sensitive_path(path: &Path) -> bool {
    let aliases = octocode_engine::text::slash_path(path);
    sensitive_directory(&aliases).is_some() || is_sensitive_file(&aliases)
}

/// The policy directory a path lies in or is (`.ssh`, `.aws`,
/// `.config/gh/`), for disclosure without the path itself.
fn sensitive_directory(aliases: &str) -> Option<&'static str> {
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
    aliases.rsplit('/').next() == Some(".git") && path.is_file()
}

fn is_sensitive_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or_default();
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
