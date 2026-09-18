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
];

pub const DISCOVERY_IGNORED_FOLDER_NAMES: &[&str] = &[
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
    ".github",
    ".vscode",
    ".devcontainer",
    ".config",
    ".cargo",
    ".yarn",
    "dist",
    "build",
    "out",
    "output",
    "target",
    "release",
    "node_modules",
    "vendor",
    "third_party",
    "tmp",
    "temp",
    "cache",
    ".cache",
    ".tmp",
    ".pytest_cache",
    ".tox",
    ".venv",
    ".mypy_cache",
    ".next",
    ".svelte-kit",
    ".turbo",
    "__pycache__",
    ".gradle",
    ".m2",
    ".idea",
    ".vs",
    ".history",
    "coverage",
    ".nyc_output",
    "DerivedData",
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

pub fn should_ignore_discovery_dir(folder: &str) -> bool {
    let normalized = folder.replace('\\', "/");
    let name = normalized
        .split('/')
        .rfind(|part| !part.is_empty())
        .unwrap_or(folder)
        .to_owned();
    DISCOVERY_IGNORED_FOLDER_NAMES.contains(&name.as_str())
}

pub fn should_ignore_discovery_file(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let name = normalized.rsplit('/').next().unwrap_or_default();
    DISCOVERY_IGNORED_FILE_NAMES.contains(&name)
        || DISCOVERY_IGNORED_FILE_EXTENSIONS
            .iter()
            .any(|extension| name.ends_with(extension))
        || normalized
            .split('/')
            .any(|part| DISCOVERY_IGNORED_FOLDER_NAMES.contains(&part))
}

pub fn should_ignore_discovery_file_with_registry(
    path: &str,
    registry: &crate::security::SecurityRegistry,
) -> bool {
    if should_ignore_discovery_file(path) {
        return true;
    }
    let normalized = path.replace('\\', "/");
    let name = normalized.rsplit('/').next().unwrap_or_default();
    registry
        .ignored_path_patterns()
        .iter()
        .any(|pattern| pattern.is_match(&normalized))
        || registry
            .ignored_file_patterns()
            .iter()
            .any(|pattern| pattern.is_match(name))
}

pub fn discovery_extension(
    path: &str,
    lowercase: bool,
    fallback: &str,
    leading_dot: bool,
) -> String {
    let name = path
        .replace('\\', "/")
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_owned();
    let extension = if let Some(dotfile) = name.strip_prefix('.') {
        dotfile
            .rsplit_once('.')
            .map_or(dotfile, |(_, extension)| extension)
    } else {
        name.rsplit_once('.')
            .map_or(fallback, |(_, extension)| extension)
    };
    let mut result = if lowercase {
        extension.to_lowercase()
    } else {
        extension.to_owned()
    };
    if leading_dot && !result.is_empty() && !result.starts_with('.') {
        result.insert(0, '.');
    }
    result
}

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
        assert!(should_ignore_discovery_dir("a/.ssh"));
        assert!(should_ignore_discovery_file("src/private-key.pem"));
        assert!(is_sensitive_path(Path::new("/tmp/work/.aws/credentials")));
        assert!(!is_sensitive_path(Path::new("/tmp/work/src/lib.rs")));
    }
}
