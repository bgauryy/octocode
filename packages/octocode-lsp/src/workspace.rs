use napi::{Error, Result, Status};
use std::path::{Path, PathBuf};

const MARKERS: [&str; 12] = [
    "package.json",
    "pnpm-workspace.yaml",
    "yarn.lock",
    "Cargo.toml",
    "go.mod",
    "pyproject.toml",
    "requirements.txt",
    "tsconfig.json",
    ".git",
    "pom.xml",
    "build.gradle",
    "Makefile",
];

pub fn resolve_workspace_root_for_file(file_path: String) -> Result<String> {
    let mut current = Path::new(&file_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(&file_path));
    loop {
        if MARKERS.iter().any(|marker| current.join(marker).exists()) {
            return Ok(current.to_string_lossy().into_owned());
        }
        if !current.pop() {
            break;
        }
    }
    std::env::current_dir()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|err| Error::new(Status::GenericFailure, err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("octocode_ws_{name}_{nanos}"));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    #[test]
    fn resolves_root_from_nearest_marker() {
        // Layout: root/package.json  root/src/index.ts
        let root = temp_dir("marker");
        fs::write(root.join("package.json"), b"{}").expect("write marker");
        let src = root.join("src");
        fs::create_dir_all(&src).expect("create src");
        let file = src.join("index.ts");
        fs::write(&file, b"export {}").expect("write file");

        let result = resolve_workspace_root_for_file(file.to_string_lossy().into_owned())
            .expect("resolve");

        let _ = fs::remove_dir_all(&root);
        // Normalize both sides in case of symlink-resolved paths (macOS /var → /private/var).
        let resolved = PathBuf::from(&result)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(&result));
        let expected = root
            .canonicalize()
            .unwrap_or(root);
        assert_eq!(resolved, expected);
    }

    #[test]
    fn resolves_root_skips_deep_nested_marker_in_favour_of_parent() {
        // Layout: root/Cargo.toml  root/a/b/file.rs
        // Nearest marker walking upward from file.rs is root/ (a/ and a/b/ have none).
        let root = temp_dir("nested");
        fs::write(root.join("Cargo.toml"), b"[package]").expect("write marker");
        let deep = root.join("a").join("b");
        fs::create_dir_all(&deep).expect("create deep");
        let file = deep.join("file.rs");
        fs::write(&file, b"fn main() {}").expect("write file");

        let result = resolve_workspace_root_for_file(file.to_string_lossy().into_owned())
            .expect("resolve");

        let _ = fs::remove_dir_all(&root);
        let resolved = PathBuf::from(&result)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(&result));
        let expected = root.canonicalize().unwrap_or(root);
        assert_eq!(resolved, expected);
    }

    #[test]
    fn falls_back_to_cwd_when_no_marker_present() {
        // A temp directory with no marker files — resolver must not panic and
        // must return *something* (cwd fallback).
        let root = temp_dir("no_marker");
        let file = root.join("orphan.ts");
        fs::write(&file, b"// nothing").expect("write file");

        let result = resolve_workspace_root_for_file(file.to_string_lossy().into_owned());
        let _ = fs::remove_dir_all(&root);
        // The resolver may return an ancestor that has a marker (the real workspace
        // root of octocode-mcp itself), or fall back to cwd.  Either way it must
        // succeed and return a non-empty string.
        let path = result.expect("resolve must not error");
        assert!(!path.is_empty());
    }
}
