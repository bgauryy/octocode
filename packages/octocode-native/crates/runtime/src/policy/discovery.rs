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

pub const DISCOVERY_IGNORED_FILE_NAMES: &[&str] = &[
    "package-lock.json",
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
    ".DS_Store",
    "Thumbs.db",
    "db.sqlite3",
];

pub const DISCOVERY_IGNORED_FILE_EXTENSIONS: &[&str] = &[
    ".lock",
    ".tmp",
    ".temp",
    ".cache",
    ".bak",
    ".backup",
    ".orig",
    ".swp",
    ".swo",
    ".rej",
    ".pid",
    ".exe",
    ".dll",
    ".so",
    ".dylib",
    ".a",
    ".lib",
    ".o",
    ".obj",
    ".bin",
    ".class",
    ".pdb",
    ".pyc",
    ".pyo",
    ".pyd",
    ".jar",
    ".war",
    ".db",
    ".sqlite",
    ".sqlite3",
    ".zip",
    ".tar",
    ".gz",
    ".bz2",
    ".xz",
    ".rar",
    ".7z",
    ".map",
    ".d.ts.map",
    ".min.js",
    ".min.css",
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
    ".patch",
    ".diff",
];

pub fn is_sensitive_path(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let aliases = normalized
        .strip_prefix("/private/tmp")
        .map(|tail| format!("/tmp{tail}"))
        .or_else(|| {
            normalized
                .strip_prefix("/private/var")
                .map(|tail| format!("/var{tail}"))
        })
        .unwrap_or(normalized);
    aliases
        .split('/')
        .any(|part| SENSITIVE_DIRECTORY_NAMES.contains(&part))
        || is_sensitive_file(&aliases)
        || aliases.contains("/.config/gcloud/")
        || aliases.contains("/.config/gh/")
        || aliases.ends_with("/.config/gh")
        || aliases.contains("/.config/hub/")
        || aliases.ends_with("/.config/hub")
        || aliases.contains("/.mozilla/firefox/")
        || aliases.contains("/Library/Keychains/")
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
            DISCOVERY_IGNORED_FILE_EXTENSIONS
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
}
