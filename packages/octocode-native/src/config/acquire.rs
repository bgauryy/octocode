use super::types::{CONFIG_FILE_NAME, ConfigInput, FileInput, RuntimeSurface};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
pub fn octocode_home(env: &BTreeMap<String, String>, cwd: &Path, os_home: &Path) -> PathBuf {
    env.get("OCTOCODE_HOME")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() { p } else { cwd.join(p) }
        })
        .unwrap_or_else(|| os_home.join(".octocode"))
}
pub fn read_file(path: PathBuf) -> FileInput {
    match fs::read_to_string(&path) {
        Ok(text) => FileInput::Read { path, text },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => FileInput::Missing { path },
        Err(e) => FileInput::Unreadable {
            path,
            kind: e.to_string(),
        },
    }
}
pub fn acquire_config_input(
    env: BTreeMap<String, String>,
    cwd: PathBuf,
    os_home: PathBuf,
    trusted_project: bool,
    runtime_surface: RuntimeSurface,
    revision: u64,
) -> ConfigInput {
    let home = octocode_home(&env, &cwd, &os_home);
    ConfigInput {
        global_env: read_file(home.join(".env")),
        project_env: read_file(cwd.join(".octocode").join(".env")),
        config_file: read_file(home.join(CONFIG_FILE_NAME)),
        env,
        cwd,
        os_home,
        trusted_project,
        runtime_surface,
        revision,
    }
}
