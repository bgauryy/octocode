use std::path::Path;

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

/// What a walk's path policy withheld: each security-policy directory name
/// with how many directories it matched, how many credential files, how
/// many `.git` pointer files, how many symlinks it did not follow, and how
/// many other denied entries (outside the allowed roots). Names come from
/// the policy lists, never the walked paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Withheld {
    pub dirs: std::collections::BTreeMap<&'static str, usize>,
    pub files: usize,
    pub(crate) git_files: usize,
    pub symlinks: usize,
    pub other: usize,
}

impl Withheld {
    /// Count one entry the path policy denied.
    pub fn record(&mut self, path: &Path, is_dir: bool) {
        if !is_sensitive_path(path) {
            if path.is_symlink() {
                self.symlinks += 1;
            } else {
                self.other += 1;
            }
            return;
        }
        let aliases = octocode_engine::text::slash_path(path);
        let own = aliases.rsplit('/').next().unwrap_or_default();
        match sensitive_directory(&aliases) {
            Some(".git") if !is_dir && own == ".git" => self.git_files += 1,
            Some(dir) if is_dir || own != dir => *self.dirs.entry(dir).or_default() += 1,
            _ => self.files += 1,
        }
    }

    pub fn total(&self) -> usize {
        self.dirs.values().sum::<usize>() + self.files + self.git_files + self.symlinks + self.other
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
        if self.git_files > 0 {
            parts.push(format!(
                "{} (submodule or worktree pointers)",
                plural(self.git_files, "`.git` file", "`.git` files")
            ));
        }
        if self.symlinks > 0 {
            parts.push(format!(
                "{} not followed",
                plural(self.symlinks, "symlink", "symlinks")
            ));
        }
        if self.other > 0 {
            parts.push(format!(
                "{} outside the allowed roots or unresolvable",
                plural(self.other, "entry", "entries")
            ));
        }
        Some(format!(
            "{} withheld by path policy: {}.",
            plural(total, "entry", "entries"),
            parts.join("; ")
        ))
    }
}

/// A text search walk's path filter: admits exactly what [`PathPolicy`]
/// admits, applies the default excludes unless `defaultExcludes:false` (the
/// text search's default-pruned directories, as [`Prune::at`] applies them
/// under the root, and the generated files), and records both, the dot and
/// ignored entries the walk left out, and the symlinks it did not follow,
/// so the result can disclose them.
///
/// [`PathPolicy`]: super::path::PathPolicy
/// [`Prune::at`]: octocode_engine::search::walk::Prune::at
pub(crate) struct SearchWalk {
    policy: super::path::PathPolicy,
    /// The searched root: a file named as the root is always searched.
    root: std::path::PathBuf,
    /// The default excludes; `None` with `defaultExcludes:false`.
    defaults: Option<octocode_engine::search::walk::RootedPrune>,
    skipped: std::sync::Mutex<WalkSkips>,
}

/// What a [`SearchWalk`] left out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct WalkSkips {
    pub withheld: Withheld,
    /// Default-pruned directories, each by its path under the walk root.
    pub pruned: std::collections::BTreeSet<String>,
    /// Default-excluded files (`*.min.js`, `.DS_Store`), each by its path
    /// under the walk root.
    pub generated: std::collections::BTreeSet<String>,
    /// Dot entries left out without `includeHidden`, each by its path under
    /// the walk root, a directory's with a trailing `/` (its contents were
    /// never walked). One the path policy withholds is not here: no flag
    /// reaches it.
    pub hidden: std::collections::BTreeSet<String>,
    /// Some dot entry would also be default-excluded (`.github/`, `.cache/`,
    /// `.DS_Store`): reaching it lifts both.
    pub(crate) hidden_excluded: bool,
    /// Entries an ignore rule left out without `includeIgnored`, each by its
    /// path under the walk root, a directory's with a trailing `/` (its
    /// contents were never walked). One the path policy withholds is not
    /// here: no flag reaches it.
    pub ignored: std::collections::BTreeSet<String>,
    /// Some ignored entry would also be default-excluded (an ignored
    /// `node_modules/`) or is a dot entry: reaching it lifts those too.
    pub(crate) ignored_excluded: bool,
    pub(crate) ignored_hidden: bool,
}

impl SearchWalk {
    /// The walk of `root` under `policy`, with the text search's default
    /// excludes (less the directories an `include` glob names) when
    /// `defaults`.
    pub fn new(
        policy: super::path::PathPolicy,
        root: std::path::PathBuf,
        defaults: bool,
        include: &[String],
    ) -> Self {
        let defaults = defaults.then(|| {
            octocode_engine::search::walk::Prune::new(
                crate::policy::prune::directories_with_includes(true, include),
            )
            .at(&root)
        });
        Self {
            policy,
            root,
            defaults,
            skipped: std::sync::Mutex::new(WalkSkips::default()),
        }
    }

    /// Whether the default excludes leave out `path`: a pruned directory
    /// or a generated file below the root (the root itself is searched).
    fn excludes(&self, path: &Path, is_dir: bool) -> bool {
        let Some(prune) = self.defaults.as_ref().filter(|_| path != self.root) else {
            return false;
        };
        if is_dir {
            prune.prunes(path, true)
        } else {
            path.file_name()
                .is_some_and(|name| generated_file_pattern(&name.to_string_lossy()).is_some())
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

impl octocode_engine::search::text_search::TextSearchPathFilter for SearchWalk {
    fn allows(&self, path: &Path, is_dir: bool) -> bool {
        if !octocode_engine::search::text_search::TextSearchPathFilter::allows(
            &self.policy,
            path,
            is_dir,
        ) {
            self.note(|skips| skips.withheld.record(path, is_dir));
            return false;
        }
        if self.excludes(path, is_dir) {
            let shown = self.shown(path);
            self.note(|skips| {
                if is_dir {
                    skips.pruned.insert(shown);
                } else {
                    skips.generated.insert(shown);
                }
            });
            return false;
        }
        true
    }

    fn skipped_hidden(&self, path: &Path, is_dir: bool) {
        if let Some((shown, excluded)) = self.skipped_entry(path, is_dir) {
            self.note(|skips| {
                skips.hidden.insert(shown);
                skips.hidden_excluded |= excluded;
            });
        }
    }

    fn skipped_ignored(&self, path: &Path, is_dir: bool) {
        if let Some((shown, excluded)) = self.skipped_entry(path, is_dir) {
            let hidden = path
                .file_name()
                .is_some_and(|name| name.as_encoded_bytes().first() == Some(&b'.'));
            self.note(|skips| {
                skips.ignored.insert(shown);
                skips.ignored_excluded |= excluded;
                skips.ignored_hidden |= hidden;
            });
        }
    }

    fn skipped_symlink(&self, _path: &Path) {
        self.note(|skips| skips.withheld.symlinks += 1);
    }
}

impl SearchWalk {
    /// An entry a walk rule (hidden, ignored) left out, as the skips record
    /// it: its path under the root (a directory's with a trailing `/`) and
    /// whether the default excludes would leave it out too; `None` for one
    /// the path policy withholds, which no flag reaches.
    fn skipped_entry(&self, path: &Path, is_dir: bool) -> Option<(String, bool)> {
        if !octocode_engine::search::text_search::TextSearchPathFilter::allows(
            &self.policy,
            path,
            is_dir,
        ) {
            return None;
        }
        let excluded = self.excludes(path, is_dir);
        let mut shown = self.shown(path);
        if is_dir {
            shown.push('/');
        }
        Some((shown, excluded))
    }

    /// `path` under the walk root, with `/` separators.
    fn shown(&self, path: &Path) -> String {
        octocode_engine::text::slash_path(path.strip_prefix(&self.root).unwrap_or(path))
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_and_access_lists_cover_sensitive_paths() {
        assert!(crate::policy::prune::defaults().any(|name| name == ".ssh"));
        assert!(
            CREDENTIAL_FILE_EXTENSIONS
                .iter()
                .any(|extension| "private-key.pem".ends_with(extension))
        );
        assert!(is_sensitive_path(Path::new("/tmp/work/.aws/credentials")));
        assert!(!is_sensitive_path(Path::new("/tmp/work/src/lib.rs")));
    }

    /// A directory named `secrets/` or `private/` is ordinary source;
    /// only a credential filename pattern withholds a file inside it.
    #[test]
    fn directory_names_never_withhold_only_credential_filenames_do() {
        for visible in [
            "/tmp/work/src/private/util.ts",
            "/tmp/work/secrets/README.md",
            "/tmp/work/src/secrets/store.ts",
            "/private/var/folders/x/work/src/lib.rs",
            "/private/tmp/work/main.rs",
        ] {
            assert!(!is_sensitive_path(Path::new(visible)), "{visible}");
        }
        for withheld in [
            "/tmp/work/secrets/prod.env",
            "/tmp/work/private/key.pem",
            "/tmp/work/private/id_rsa",
            "/tmp/work/secrets/.env.local",
            "/tmp/work/private/release.keystore",
            "/tmp/work/secrets/credentials.json",
        ] {
            assert!(is_sensitive_path(Path::new(withheld)), "{withheld}");
            assert_eq!(sensitive_directory(withheld), None, "{withheld}");
            assert!(
                withheld_message("x", Path::new(withheld)).contains("a credential file name"),
                "{withheld}"
            );
        }
    }

    /// Read, tree and search share one credential list, so every name
    /// the search excludes is withheld from reads too, and the stores the
    /// lists used to miss are covered.
    #[test]
    fn one_credential_list_covers_read_and_search() {
        for name in CREDENTIAL_FILE_NAMES {
            assert!(
                is_sensitive_path(&Path::new("/tmp/work").join(name)),
                "{name}"
            );
        }
        for name in [
            "secrets.json",
            "credentials.json",
            "service-account.json",
            "auth.json",
            "api-keys.json",
            "keyfile",
            "google-services.json",
            ".pgpass",
            ".htpasswd",
            ".vault-token",
            ".s3cfg",
            ".boto",
            ".my.cnf",
            ".dockercfg",
            ".envrc",
            ".flaskenv",
            "app.env",
            "config.env",
            ".env.local",
            "id_rsa.old",
            "deploy.ppk",
            "credentials.json.bak",
        ] {
            assert!(
                is_sensitive_path(&Path::new("/tmp/work").join(name)),
                "{name}"
            );
        }
        for name in [
            "environment.ts",
            "env.rs",
            ".envoy.yaml",
            "keys.rs",
            "auth.ts",
        ] {
            assert!(
                !is_sensitive_path(&Path::new("/tmp/work").join(name)),
                "{name}"
            );
        }
    }

    /// The notice names what it withheld: a `.git` pointer file is not
    /// a credential file, and a symlink the walk did not follow is not one
    /// outside the allowed roots.
    #[cfg(unix)]
    #[test]
    fn withheld_notice_labels_git_files_and_unfollowed_symlinks() {
        let root = tempfile::tempdir().expect("root");
        let sub = root.path().join("sub");
        std::fs::create_dir_all(sub.join("real")).expect("dirs");
        std::fs::write(sub.join(".git"), "gitdir: ../.git/modules/sub\n").expect("gitfile");
        std::os::unix::fs::symlink(sub.join("real"), sub.join("link")).expect("link");
        let mut withheld = Withheld::default();
        withheld.record(&sub.join(".git"), false);
        withheld.record(&sub.join("link"), false);
        withheld.record(&root.path().join("gone"), false);
        let notice = withheld.notice().expect("notice");
        assert_eq!(
            notice,
            "3 entries withheld by path policy: 1 `.git` file (submodule or worktree pointers); 1 symlink not followed; 1 entry outside the allowed roots or unresolvable.",
        );
        assert!(
            withheld_message("sub/.git", &sub.join(".git")).contains("a `.git` file"),
            "the read refusal names the pointer file too"
        );
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
