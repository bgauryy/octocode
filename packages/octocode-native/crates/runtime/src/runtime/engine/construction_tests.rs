use super::*;
use octocode_runtime_base::config::FileInput;
use octocode_tools_shared::source_capture::SourceReader;
use secrecy::SecretString;
mod construct;
mod runtime_state;
mod warm_run;

/// An MCP runtime at `home` with the local section on, which an MCP host
/// turns on itself; `env` overrides it and adds the rest.
pub(super) fn runtime(home: &std::path::Path, env: &[(&str, &str)]) -> ToolRuntime {
    let local = octocode_runtime_base::config::field::LOCAL_ENABLED
        .env_name()
        .expect("local.enabled has a variable");
    let mut vars: BTreeMap<String, String> = [(local, "true")]
        .iter()
        .chain(env)
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect();
    vars.insert("OCTOCODE_HOME".into(), home.to_string_lossy().into_owned());
    ToolRuntime::new(octocode_runtime_base::config::ConfigInput {
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
