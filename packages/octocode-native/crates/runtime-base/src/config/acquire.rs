use super::types::{CONFIG_FILE_NAME, ConfigInput, FileInput, RuntimeSurface};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
pub(crate) fn octocode_home(env: &BTreeMap<String, String>, cwd: &Path, os_home: &Path) -> PathBuf {
    env.get(super::environment::OCTOCODE_HOME)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let p = PathBuf::from(s);
            if p.is_absolute() { p } else { cwd.join(p) }
        })
        .unwrap_or_else(|| os_home.join(".octocode"))
}
/// Read one configuration layer, bounded like every config edit. Symlinks are
/// followed: a dotfile manager may link the file into place.
pub(crate) fn read_file(path: PathBuf) -> FileInput {
    let read = fs::File::open(&path)
        .and_then(|file| crate::private_file::read_limited(file, super::edit::MAX_CONFIG_BYTES))
        .and_then(|bytes| {
            String::from_utf8(bytes)
                .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "not UTF-8 text"))
        });
    match read {
        Ok(text) => FileInput::Read { path, text },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => FileInput::Missing { path },
        Err(e) => FileInput::Unreadable {
            path,
            kind: e.to_string(),
        },
    }
}
fn same_file(a: &Path, b: &Path) -> bool {
    a == b
        || matches!(
            (fs::canonicalize(a), fs::canonicalize(b)),
            (Ok(a), Ok(b)) if a == b
        )
}
/// A workspace layer file, unless it is the home file itself: when cwd is the
/// OS home, `<cwd>/.octocode` is the Octocode home and its files are the home
/// layers only, never read a second time as workspace layers.
fn workspace_file(path: PathBuf, home_file: &Path) -> FileInput {
    if same_file(&path, home_file) {
        FileInput::Missing { path }
    } else {
        read_file(path)
    }
}
pub fn acquire_config_input(
    env: BTreeMap<String, String>,
    cwd: PathBuf,
    os_home: PathBuf,
    runtime_surface: RuntimeSurface,
) -> ConfigInput {
    let home = octocode_home(&env, &cwd, &os_home);
    let workspace = cwd.join(".octocode");
    let global_env = home.join(".env");
    let global_config = home.join(CONFIG_FILE_NAME);
    ConfigInput {
        project_env: workspace_file(workspace.join(".env"), &global_env),
        project_config_file: workspace_file(workspace.join(CONFIG_FILE_NAME), &global_config),
        global_env: read_file(global_env),
        config_file: read_file(global_config),
        env,
        cwd,
        os_home,
        runtime_surface,
    }
}
