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
