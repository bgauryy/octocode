use std::path::Path;

pub const SENSITIVE_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    ".ssh",
    ".aws",
    ".docker",
    ".azure",
    ".kube",
    ".terraform",
    "secrets",
    "private",
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

/// Credential-shaped file names a text search never reads, whatever its
/// flags (the read policy withholds most of them too).
pub const CREDENTIAL_FILE_NAMES: &[&str] = &[
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

/// Credential-shaped file extensions a text search never reads.
pub const CREDENTIAL_FILE_EXTENSIONS: &[&str] = &[
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
];

/// Generated, lock, and OS-metadata file names a text search skips by
/// default; `defaultExcludes:false` searches them.
pub const GENERATED_FILE_NAMES: &[&str] =
    &["package-lock.json", ".DS_Store", "Thumbs.db", "db.sqlite3"];

/// Generated, minified, archive, and object-file extensions a text search
/// skips by default; `defaultExcludes:false` searches them. Source maps are
/// named by their compound suffix, so a keymap or linker `.map` source stays
/// searchable.
pub const GENERATED_FILE_EXTENSIONS: &[&str] = &[
    ".lock", ".tmp", ".temp", ".cache", ".bak", ".backup", ".orig", ".swp", ".swo", ".rej", ".pid",
    ".exe", ".dll", ".so", ".dylib", ".a", ".lib", ".o", ".obj", ".bin", ".class", ".pdb", ".pyc",
    ".pyo", ".pyd", ".jar", ".war", ".db", ".sqlite", ".sqlite3", ".zip", ".tar", ".gz", ".bz2",
    ".xz", ".rar", ".7z", ".js.map", ".mjs.map", ".cjs.map", ".css.map", ".ts.map", ".min.js",
    ".min.css", ".patch", ".diff",
];

/// The default-exclude pattern a file name matches (`package-lock.json`,
/// `*.min.js`), for a count by pattern.
pub fn generated_file_pattern(name: &str) -> Option<String> {
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

/// `path` with `/` separators and macOS's `/private/tmp` and `/private/var`
/// spelled as their `/tmp` and `/var` aliases, so the system `private`
/// directory never reads as a sensitive one.
fn normalized(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    normalized
        .strip_prefix("/private/tmp")
        .map(|tail| format!("/tmp{tail}"))
        .or_else(|| {
            normalized
                .strip_prefix("/private/var")
                .map(|tail| format!("/var{tail}"))
        })
        .unwrap_or(normalized)
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

pub fn is_sensitive_path(path: &Path) -> bool {
    let aliases = normalized(path);
    sensitive_directory(&aliases).is_some() || is_sensitive_file(&aliases)
}

/// The policy directory a path lies in or is (`secrets`, `.ssh`,
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
pub const WITHHELD_HINT: &str = "Do not retry or respell: the security path policy withholds this path; no flag or config lifts it.";

/// The error message for a requested path the security policy withholds:
/// what it matched, and that nothing lifts it.
pub fn withheld_message(display: &str, path: &Path) -> String {
    let matched = match sensitive_directory(&normalized(path)) {
        Some(dir) => format!("a `{}/` directory", dir.trim_end_matches('/')),
        None => "a credential file name".to_owned(),
    };
    format!(
        "Path '{display}' is withheld by the security path policy ({matched}): no local tool reads it, and no flag or config setting lifts it."
    )
}

/// What a walk's path policy withheld: each security-policy directory name
/// with how many directories it matched, how many credential files, and
/// how many other denied entries (symlinks leaving the allowed roots or not
/// resolving). Names come from the policy lists, never the walked paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Withheld {
    pub dirs: std::collections::BTreeMap<&'static str, usize>,
    pub files: usize,
    pub other: usize,
}

impl Withheld {
    /// Count one entry the path policy denied.
    pub fn record(&mut self, path: &Path, is_dir: bool) {
        if !is_sensitive_path(path) {
            self.other += 1;
            return;
        }
        let aliases = normalized(path);
        let own = aliases.rsplit('/').next().unwrap_or_default();
        match sensitive_directory(&aliases) {
            Some(dir) if is_dir || own != dir => *self.dirs.entry(dir).or_default() += 1,
            _ => self.files += 1,
        }
    }

    pub fn total(&self) -> usize {
        self.dirs.values().sum::<usize>() + self.files + self.other
    }

    /// One disclosure: the count, which security-policy directories, that
    /// no flag or config setting lifts the policy, and that absence there
    /// is unproven.
    pub fn notice(&self) -> Option<String> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let plural = |count: usize, one: &str, many: &str| {
            format!("{count} {}", if count == 1 { one } else { many })
        };
        let mut parts = Vec::new();
        if !self.dirs.is_empty() {
            let names = self
                .dirs
                .iter()
                .map(|(name, count)| {
                    let name = name.trim_end_matches('/');
                    if *count > 1 {
                        format!("{name}/ ×{count}")
                    } else {
                        format!("{name}/")
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("security-policy dirs {names}"));
        }
        if self.files > 0 {
            parts.push(plural(self.files, "credential file", "credential files"));
        }
        if self.other > 0 {
            parts.push(format!(
                "{} outside the allowed roots or unresolvable",
                plural(self.other, "symlink", "symlinks")
            ));
        }
        Some(format!(
            "{} withheld by path policy: {}. No flag or config setting lifts it; absence there is unproven.",
            plural(total, "entry", "entries"),
            parts.join("; ")
        ))
    }
}

/// A text search walk's path filter: admits exactly what [`PathPolicy`]
/// admits, skips the default-excluded generated files unless
/// `defaultExcludes:false`, and counts both so the result can disclose them.
///
/// [`PathPolicy`]: super::path::PathPolicy
pub struct SearchWalk {
    policy: super::path::PathPolicy,
    /// The searched root: a file named as the root is always searched.
    root: std::path::PathBuf,
    skip_generated: bool,
    skipped: std::sync::Mutex<WalkSkips>,
}

/// What a [`SearchWalk`] left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WalkSkips {
    pub withheld: Withheld,
    /// Default-excluded files by pattern (`*.lock`, `package-lock.json`).
    pub generated: std::collections::BTreeMap<String, usize>,
}

impl SearchWalk {
    pub fn new(
        policy: super::path::PathPolicy,
        root: std::path::PathBuf,
        skip_generated: bool,
    ) -> Self {
        Self {
            policy,
            root,
            skip_generated,
            skipped: std::sync::Mutex::new(WalkSkips::default()),
        }
    }

    pub fn skipped(&self) -> WalkSkips {
        self.skipped
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    fn note(&self, record: impl FnOnce(&mut WalkSkips)) {
        record(
            &mut self
                .skipped
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
    }
}

impl octocode_engine::portable::RipgrepPathFilter for SearchWalk {
    fn allows(&self, path: &Path, is_dir: bool) -> bool {
        if !octocode_engine::portable::RipgrepPathFilter::allows(&self.policy, path, is_dir) {
            self.note(|skips| skips.withheld.record(path, is_dir));
            return false;
        }
        if self.skip_generated
            && !is_dir
            && path != self.root
            && let Some(pattern) = path
                .file_name()
                .and_then(|name| generated_file_pattern(&name.to_string_lossy()))
        {
            self.note(|skips| *skips.generated.entry(pattern).or_default() += 1);
            return false;
        }
        true
    }
}

fn is_sensitive_file(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or_default();
    name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name,
            ".npmrc"
                | ".pypirc"
                | ".netrc"
                | "credentials"
                | ".credentials"
                | "known_hosts"
                | "authorized_keys"
                | "kubeconfig"
                | "terraform.tfstate"
                | "terraform.tfvars"
                | "master.key"
                | ".git-credentials"
                | ".bash_history"
                | ".zsh_history"
                | "shadow"
                | "gshadow"
                | "wallet.dat"
        )
        || [
            ".pem", ".key", ".crt", ".cer", ".p12", ".pfx", ".jks", ".ppk", ".kdbx", ".gpg",
            ".asc", ".ovpn",
        ]
        .iter()
        .any(|extension| name.ends_with(extension))
        || matches!(name, "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_and_access_lists_cover_sensitive_paths() {
        assert!(
            crate::policy::prune::PruneMode::SyntaxVisible
                .defaults()
                .any(|name| name == ".ssh")
        );
        assert!(
            CREDENTIAL_FILE_EXTENSIONS
                .iter()
                .any(|extension| "private-key.pem".ends_with(extension))
        );
        assert!(is_sensitive_path(Path::new("/tmp/work/.aws/credentials")));
        assert!(!is_sensitive_path(Path::new("/tmp/work/src/lib.rs")));
    }

    #[test]
    fn access_list_covers_gh_and_gpg_credentials() {
        assert!(is_sensitive_path(Path::new(
            "/home/user/.config/gh/hosts.yml"
        )));
        assert!(is_sensitive_path(Path::new("/home/user/.config/gh")));
        assert!(is_sensitive_path(Path::new("/home/user/.config/hub")));
        assert!(is_sensitive_path(Path::new(
            "/home/user/.gnupg/private-keys-v1.d/anything"
        )));
        assert!(is_sensitive_path(Path::new(
            "/home/user/.docker/config.json"
        )));
    }

    #[test]
    fn keymap_sources_are_not_source_maps() {
        assert_eq!(generated_file_pattern("speakupmap.map"), None);
        assert_eq!(
            generated_file_pattern("app.js.map").as_deref(),
            Some("*.js.map")
        );
        assert_eq!(
            generated_file_pattern("index.d.ts.map").as_deref(),
            Some("*.ts.map")
        );
        assert_eq!(
            generated_file_pattern("site.min.css").as_deref(),
            Some("*.min.css")
        );
    }
}
