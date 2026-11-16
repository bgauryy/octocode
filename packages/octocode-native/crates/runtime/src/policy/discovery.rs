use std::borrow::Cow;
use std::path::{Path, PathBuf};

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

/// The file names credential stores are written under: no local tool reads,
/// lists or searches them, whatever its flags. The one list behind
/// [`is_sensitive_path`] (read and tree) and the text search's path filter;
/// the dotenv family ([`is_dotenv_name`]) sits beside it. Lowercase, as every
/// name is compared [`folded`]. Any other file, shell history and rc files
/// included, is the agent's to read: its known secrets are redacted.
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
    ".git-credentials",
    "credentials",
    ".credentials",
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
    ".credentials.json",
    "credentials.yaml",
    "credentials.yml",
    // Cargo (1.46+) writes `~/.cargo/credentials.toml`; Terraform writes
    // `credentials.tfrc.json` under `~/.terraform.d`.
    "credentials.toml",
    "credentials.tfrc.json",
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
    // The key beside Octocode's encrypted `credentials.json`: the extension
    // rule needs a stem, so this bare name would slip past it.
    ".key",
    "gcloud-service-key.json",
    "firebase-adminsdk.json",
    "google-services.json",
    "googleservice-info.plist",
    // Windows and system spellings of the same stores: curl and git read
    // `_netrc` on Windows, PostgreSQL `pgpass.conf` under %APPDATA%, npm a
    // global `npmrc`, Apache tools write `htpasswd` without the dot.
    "_netrc",
    "pgpass.conf",
    "npmrc",
    "htpasswd",
    // The kubeconfig k3s and RKE2 write (`/etc/rancher/k3s/k3s.yaml`).
    "k3s.yaml",
    "rke2.yaml",
];

/// SSH private keys by the type `ssh-keygen` names them with, under any
/// suffix a person adds (`id_ed25519_github`, `id_ecdsa_sk`); a name with an
/// extension after it (`id_rsa.pub`, `id_rsa_loader.rs`) is not one.
const SSH_KEY_PREFIXES: &[&str] = &["id_rsa", "id_dsa", "id_ecdsa", "id_ed25519"];

/// The kubeconfigs kubeadm writes in `/etc/kubernetes/`: generic names, so
/// withheld only in a directory named `kubernetes` (an `admin.conf` elsewhere
/// is ordinary config).
const KUBEADM_CONFIGS: &[&str] = &[
    "admin.conf",
    "super-admin.conf",
    "kubelet.conf",
    "controller-manager.conf",
    "scheduler.conf",
];

/// The name endings of credential stores: key and keystore formats, and the
/// files a console downloads a credential as. Shared like
/// [`CREDENTIAL_FILE_NAMES`]. A backup suffix (`id_rsa.old`) is matched by
/// [`is_sensitive_path`] on the stem; `*.env` is the dotenv rule's.
/// Certificates and public keys (`.crt`, `.asc`) are public: they read.
pub(crate) const CREDENTIAL_FILE_EXTENSIONS: &[&str] = &[
    ".key",
    ".pem",
    ".p12",
    ".pfx",
    ".jks",
    ".keystore",
    ".ppk",
    ".kdbx",
    ".gpg",
    ".ovpn",
    // Terraform state holds every resource's secrets in plain text, and a
    // variables file is where Terraform's docs put the secrets it is given.
    ".tfstate",
    ".tfvars",
    // `admin.kubeconfig` and the like.
    ".kubeconfig",
    // Terraform loads JSON variable files as it loads `.tfvars`.
    ".tfvars.json",
    // AWS's IAM console download (`<user>_accessKeys.csv`) and Google's OAuth
    // client download (`client_secret_<id>.apps.googleusercontent.com.json`).
    "_accesskeys.csv",
    ".apps.googleusercontent.com.json",
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

/// [`is_sensitive_path`] of a file a repository names by `path`, the rule the
/// classification egress applies whatever the source. The path is rooted, as a
/// local one is, so a location named by a fragment (`.config/gh/`) matches at
/// the repository root too, and read as GitHub reads it (`./.env`, `a/../.env`).
pub(crate) fn is_sensitive_repository_file(path: &str) -> bool {
    is_sensitive_path(&repository_path(path), EntryKind::File)
}

/// A repository's `path` rooted as a local path is, the form every
/// repository-file rule judges. A `\` is read as a separator on every
/// platform, failing closed: git allows it in a name, and a Windows checkout
/// or materialize splits on it (`a\.ssh\config` lands as `a/.ssh/config`).
/// A local Unix path keeps `\` as a name byte ([`octocode_engine::text::slash_path`]).
pub(crate) fn repository_path(path: &str) -> PathBuf {
    let separated = path.replace('\\', "/");
    Path::new("/").join(octocode_engine::text::lexical(Path::new(&separated)))
}

/// `path` as a case-insensitive filesystem compares it ([`folded`]).
pub(crate) fn folded_path(path: &Path) -> PathBuf {
    PathBuf::from(folded(&octocode_engine::text::slash_path(path)).into_owned())
}

/// Whether `path` is or lies in a sensitive directory, which no setting lifts.
pub(crate) fn is_in_sensitive_directory(path: &Path) -> bool {
    sensitive_directory(&octocode_engine::text::slash_path(path)).is_some()
}

/// [`is_sensitive_path`] of an entry named `name` under a directory the check
/// already cleared: only the name can add a reason. `None` when the whole path
/// must tell: a name that can complete a credential location
/// ([`SENSITIVE_FRAGMENTS`]), or one that is not text.
pub(crate) fn is_sensitive_under_cleared(name: &std::ffi::OsStr, kind: EntryKind) -> Option<bool> {
    let name = folded(name.to_str()?);
    // A name whose rule needs its directory (a kubeadm config) is judged on the
    // whole path too.
    let completes = KUBEADM_CONFIGS.contains(&name.as_ref())
        || SENSITIVE_FRAGMENTS.iter().any(|(fragment, _)| {
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

/// Recovery for a path the security policy withholds: no query field lifts it,
/// so a retry or a respelling cannot succeed.
pub(crate) const WITHHELD_HINT: &str = "Do not retry or respell: the security path policy withholds this path, and no query field lifts it.";

/// The error message for a requested path the security policy withholds: what
/// it matched, and what can lift it. A credential file name depends on the
/// user's configuration and the tool; a sensitive directory never opens.
pub(crate) fn withheld_message(display: &str, path: &Path) -> String {
    let aliases = octocode_engine::text::slash_path(path);
    let matched = match sensitive_directory(&aliases) {
        Some(".git") if is_git_file(path, &aliases) => {
            "a `.git` file, the pointer a submodule or worktree keeps to its repository".to_owned()
        }
        Some(dir) => format!("a `{}/` directory", dir.trim_end_matches('/')),
        None => {
            return format!(
                "Path '{display}' is withheld by the security path policy (a credential file name): this tool does not read it under the user's configuration."
            );
        }
    };
    format!(
        "Path '{display}' is withheld by the security path policy ({matched}): no local tool reads it, and no flag or config setting lifts it."
    )
}

/// What a link points to when the policy withholds its target, named by its
/// kind only: the caller asked for the link, never for the withheld name.
pub(crate) fn withheld_link_message(display: &str, target: &Path) -> String {
    let aliases = octocode_engine::text::slash_path(target);
    let what = match sensitive_directory(&aliases) {
        Some(".git") if is_git_file(target, &aliases) => "a `.git` file".to_owned(),
        Some(dir) => format!("an entry in a `{}/` directory", dir.trim_end_matches('/')),
        None => "a credential file".to_owned(),
    };
    format!(
        "Path '{display}' links to {what}, which the security path policy withholds: no local tool reads it through a link."
    )
}

/// A `.git` entry that is a file (a submodule's or linked worktree's
/// `gitdir:` pointer), not the repository directory.
fn is_git_file(path: &Path, aliases: &str) -> bool {
    aliases.rsplit('/').next().map(folded).as_deref() == Some(".git") && path.is_file()
}

fn is_sensitive_file(path: &str) -> bool {
    let folded_path = folded(path);
    let mut parts = folded_path.rsplit('/');
    let raw = parts.next().unwrap_or_default();
    let parent = parts.next().unwrap_or_default();
    // Windows opens a name without its trailing dots and spaces
    // (`kubeconfig.yaml.` is `kubeconfig.yaml`). A read is judged again on the
    // canonical path ([`super::path`]), which on Windows carries the long name
    // an 8.3 short name stands for.
    let trimmed = raw.trim_end_matches(['.', ' ']);
    let name = if trimmed.is_empty() { raw } else { trimmed };
    (parent == "kubernetes" && KUBEADM_CONFIGS.contains(&name))
        || is_credential_name(name)
        || backup_of(name).is_some_and(is_credential_name)
}

/// The name a backup copy `name` was made from (`id_rsa.old`, `.env.bak`).
fn backup_of(name: &str) -> Option<&str> {
    BACKUP_SUFFIXES
        .iter()
        .find_map(|suffix| name.strip_suffix(suffix).filter(|stem| !stem.is_empty()))
}

/// Whether `name` (folded) is a dotenv file: `.env`, `.env.*`, `.envrc`,
/// `.flaskenv` or `*.env`.
pub(crate) fn is_dotenv_name(name: &str) -> bool {
    name == ".env"
        || name.starts_with(".env.")
        || name == ".envrc"
        || name == ".flaskenv"
        || (name.len() > ".env".len() && name.ends_with(".env"))
}

/// Whether `name` is a credential store's: a dotenv file, a listed name or
/// ending, or an SSH private key.
fn is_credential_name(name: &str) -> bool {
    is_dotenv_name(name)
        || CREDENTIAL_FILE_NAMES.contains(&name)
        || CREDENTIAL_FILE_EXTENSIONS
            .iter()
            .any(|extension| name.len() > extension.len() && name.ends_with(extension))
        || SSH_KEY_PREFIXES.iter().any(|prefix| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| !rest.contains('.'))
        })
}
