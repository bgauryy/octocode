use super::*;
use crate::config::{ConfigInput, FileInput};
use std::collections::BTreeMap;

fn availability(env: &[(&str, &str)], surface: RuntimeSurface) -> ToolAvailability {
    availability_with(env, surface, ToolId::is_disabled)
}

fn availability_with(
    env: &[(&str, &str)],
    surface: RuntimeSurface,
    disabled: impl Fn(ToolId) -> bool,
) -> ToolAvailability {
    let home = std::path::PathBuf::from("/synthetic/home");
    let mut vars: BTreeMap<String, String> = env
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect();
    vars.insert("OCTOCODE_HOME".into(), "/synthetic/home/.octocode".into());
    let input = ConfigInput {
        env: vars,
        cwd: "/synthetic/cwd".into(),
        os_home: home.clone(),
        global_env: FileInput::Missing {
            path: home.join(".env"),
        },
        project_env: FileInput::Missing {
            path: "/synthetic/cwd/.octocode/.env".into(),
        },
        config_file: FileInput::Missing {
            path: home.join(".octocoderc"),
        },
        project_config_file: FileInput::Missing {
            path: "/synthetic/cwd/.octocode/.octocoderc".into(),
        },
        runtime_surface: surface,
    };
    ToolAvailability::resolve_with(&crate::config::resolve_config(&input), surface, disabled)
}

/// A tool the contract parks is off on every surface, whatever the
/// beta gate, local switch, or tool list says, and names why.
#[test]
fn a_disabled_tool_is_off_on_every_surface_whatever_the_configuration() {
    let parked = |id: ToolId| id == ToolId::AstSearch;
    for surface in [RuntimeSurface::Cli, RuntimeSurface::Mcp] {
        let tools = availability_with(
            &[
                ("OCTOCODE_BETA", "true"),
                ("TOOLS_TO_RUN", "astSearch,localSearch"),
            ],
            surface,
            parked,
        );
        let search = tools.get("astSearch").expect("known");
        assert_eq!(search, &Availability::Disabled, "{surface:?}");
        assert!(!tools.is_available("astSearch"));
        assert!(!tools.available().contains(&"astSearch"));
        assert_eq!(search.reason_code(), Some("disabled"));
        assert_eq!(search.reason_text(), "it is disabled in this build.");
        assert!(search.env_vars().is_empty());
        assert!(!search.needs_opt_in());
        assert!(tools.is_available("localSearch"));
    }
}

/// The generated policy and the contract agree tool by tool.
#[test]
fn the_generated_disabled_policy_follows_the_contract() {
    for id in ToolId::ALL {
        let tool = crate::contracts::tool_contract(id).expect("tool contract");
        assert_eq!(
            id.is_disabled(),
            tool.get("disabled").and_then(serde_json::Value::as_bool) == Some(true),
            "{id}"
        );
        if id.is_disabled() {
            let tools = availability(&[("OCTOCODE_BETA", "true")], RuntimeSurface::Cli);
            assert_eq!(
                tools.get(id.as_str()),
                Some(&Availability::Disabled),
                "{id}"
            );
        }
    }
}

#[test]
fn each_tool_has_one_reason_with_its_variables() {
    let cli = availability(
        &[
            ("OCTOCODE_ENABLE_LOCAL", "false"),
            ("DISABLE_TOOLS", "ghSearchRepo"),
        ],
        RuntimeSurface::Cli,
    );
    let local = cli.get("localSearch").expect("known");
    assert_eq!(local.reason_code(), Some("missingConfiguration"));
    assert_eq!(local.env_vars(), ["OCTOCODE_ENABLE_LOCAL"]);
    assert_eq!(
        local.reason_text(),
        "it needs local.enabled (OCTOCODE_ENABLE_LOCAL)."
    );
    let listed = cli.get("ghSearchRepo").expect("known");
    assert_eq!(listed.reason_code(), Some("toolsList"));
    assert_eq!(
        listed.reason_text(),
        "tools.disabled (DISABLE_TOOLS) lists it."
    );
    assert_eq!(listed.env_vars(), ["DISABLE_TOOLS"]);
    let clasify = cli.get("clasify").expect("known");
    assert_eq!(clasify.env_vars(), ["OCTOCODE_CLASSIFICATION_API"]);
    assert!(clasify.needs_opt_in());
    assert!(cli.get("noSuchTool").is_none());
    assert!(!cli.is_available("noSuchTool"));
}

#[test]
fn the_tool_list_outranks_the_surface_and_settings() {
    let mcp = availability(&[("TOOLS_TO_RUN", "localSearch")], RuntimeSurface::Mcp);
    assert_eq!(
        mcp.get("ghCloneRepo").and_then(Availability::reason_code),
        Some("toolsList")
    );
    assert_eq!(
        mcp.get("ghSearchCode")
            .map(Availability::reason_text)
            .as_deref(),
        Some("tools.enabled (TOOLS_TO_RUN) does not list it.")
    );
    let all = availability(&[], RuntimeSurface::Mcp);
    assert_eq!(all.get("ghCloneRepo"), Some(&Availability::CliOnly));
    assert!(!all.is_available("ghCloneRepo"));
    let cli = availability(&[], RuntimeSurface::Cli);
    assert!(cli.is_available("ghCloneRepo"));
    let memory = availability(&[("OCTOCODE_STORAGE_MODE", "memory")], RuntimeSurface::Cli);
    assert_eq!(
        memory.get("ghCloneRepo").map(Availability::env_vars),
        Some(vec!["OCTOCODE_STORAGE_MODE"])
    );
}

#[test]
fn a_blank_classification_key_opts_out() {
    let blank = availability(
        &[
            ("OCTOCODE_BETA", "true"),
            ("OCTOCODE_CLASSIFICATION_API", "  "),
        ],
        RuntimeSurface::Cli,
    );
    assert!(!blank.is_available("clasify"));
    let set = availability(
        &[
            ("OCTOCODE_BETA", "false"),
            ("OCTOCODE_CLASSIFICATION_API", "k"),
        ],
        RuntimeSurface::Cli,
    );
    assert!(set.is_available("clasify"));
}

#[test]
fn beta_graph_tools_preserve_cli_access_and_require_opt_in_on_mcp() {
    for beta in ["false", "true"] {
        for surface in [RuntimeSurface::Cli, RuntimeSurface::Mcp] {
            let tools = availability(&[("OCTOCODE_BETA", beta)], surface);
            for name in ["graphBuild", "graphQuery"] {
                let expected = surface == RuntimeSurface::Cli || beta == "true";
                assert_eq!(
                    tools.is_available(name),
                    expected,
                    "{name} {surface:?} {beta}"
                );
                if !expected {
                    let reason = tools.get(name).expect("known graph tool");
                    assert_eq!(reason.env_vars(), vec!["OCTOCODE_BETA"]);
                    assert_eq!(reason.reason_code(), Some("missingConfiguration"));
                }
            }
        }
    }
    for env in [
        vec![
            ("OCTOCODE_BETA", "true"),
            ("OCTOCODE_ENABLE_LOCAL", "false"),
        ],
        vec![("OCTOCODE_BETA", "true"), ("TOOLS_TO_RUN", "localFetch")],
    ] {
        for surface in [RuntimeSurface::Cli, RuntimeSurface::Mcp] {
            let tools = availability(&env, surface);
            assert!(!tools.is_available("graphBuild"));
            assert!(!tools.is_available("graphQuery"));
        }
    }
}
