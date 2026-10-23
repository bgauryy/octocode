use super::*;
use crate::config::FileInput;
use crate::tools::source_capture::SourceReader;
use secrecy::SecretString;
mod construct;
mod runtime_state;

pub(super) fn runtime(home: &std::path::Path, env: &[(&str, &str)]) -> ToolRuntime {
    let mut vars: BTreeMap<String, String> = env
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect();
    vars.insert("OCTOCODE_HOME".into(), home.to_string_lossy().into_owned());
    ToolRuntime::new(crate::config::ConfigInput {
        env: vars,
        cwd: home.to_path_buf(),
        os_home: home.to_path_buf(),
        global_env: FileInput::Missing {
            path: home.join(".env"),
        },
        project_env: FileInput::Missing {
            path: home.join(".octocode/.env"),
        },
        config_file: FileInput::Missing {
            path: home.join(".octocoderc"),
        },
        project_config_file: FileInput::Missing {
            path: home.join(".octocode/.octocoderc"),
        },
        runtime_surface: RuntimeSurface::Mcp,
    })
    .expect("runtime")
}
