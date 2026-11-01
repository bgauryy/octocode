use super::*;
use crate::config::{ConfigInput, FileInput};
use std::collections::BTreeMap;

#[test]
fn cli_catalog_keeps_all_tools_when_mcp_settings_and_prerequisites_are_off() {
    for env in [
        vec![],
        vec![
            ("OCTOCODE_ENABLE_LOCAL", "false"),
            ("OCTOCODE_BETA", "false"),
        ],
        vec![
            ("TOOLS_TO_RUN", "localFetch"),
            ("DISABLE_TOOLS", "localFetch,clasify"),
        ],
        vec![
            ("OCTOCODE_STORAGE_MODE", "memory"),
            ("OCTOCODE_CLASSIFICATION_API", "  "),
            ("OCTOCODE_ENABLE_CLONE", "false"),
        ],
    ] {
        let tools = availability(&env, RuntimeSurface::Cli);
        for id in ToolId::ALL {
            assert!(tools.is_available(id.as_str()), "{id}: {env:?}");
        }
    }
}

#[test]
fn mcp_clone_opt_in_is_independent_of_beta_key_and_storage() {
    for enabled in ["false", "true"] {
        for beta in ["false", "true"] {
            for storage in ["memory", "persistent"] {
                let tools = availability(
                    &[
                        ("OCTOCODE_ENABLE_CLONE", enabled),
                        ("OCTOCODE_BETA", beta),
                        ("OCTOCODE_STORAGE_MODE", storage),
                        ("OCTOCODE_CLASSIFICATION_API", "fixture-key"),
                    ],
                    RuntimeSurface::Mcp,
                );
                assert_eq!(
                    tools.is_available("ghCloneRepo"),
                    enabled == "true",
                    "{enabled} {beta} {storage}"
                );
                if enabled == "false" {
                    assert_eq!(
                        tools.get("ghCloneRepo").unwrap().env_vars(),
                        ["OCTOCODE_ENABLE_CLONE"]
                    );
                }
            }
        }
    }
}

fn availability(env: &[(&str, &str)], surface: RuntimeSurface) -> ToolAvailability {
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
    ToolAvailability::resolve(&crate::config::resolve_config(&input), surface)
}

#[test]
fn each_tool_has_one_reason_with_its_variables() {
    let cli = availability(
        &[
            ("OCTOCODE_ENABLE_LOCAL", "false"),
            ("DISABLE_TOOLS", "ghSearchRepo"),
        ],
        RuntimeSurface::Mcp,
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
    assert_eq!(
        all.get("ghCloneRepo").unwrap().env_vars(),
        ["OCTOCODE_ENABLE_CLONE"]
    );
    assert!(!all.is_available("ghCloneRepo"));
    let cli = availability(&[], RuntimeSurface::Cli);
    assert!(cli.is_available("ghCloneRepo"));
    let memory = availability(&[("OCTOCODE_STORAGE_MODE", "memory")], RuntimeSurface::Cli);
    assert!(memory.is_available("ghCloneRepo"));
}

#[test]
fn a_blank_classification_key_opts_out() {
    let blank = availability(
        &[
            ("OCTOCODE_BETA", "true"),
            ("OCTOCODE_CLASSIFICATION_API", "  "),
        ],
        RuntimeSurface::Mcp,
    );
    assert!(!blank.is_available("clasify"));
    let set = availability(
        &[
            ("OCTOCODE_BETA", "false"),
            ("OCTOCODE_CLASSIFICATION_API", "k"),
        ],
        RuntimeSurface::Mcp,
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
            assert_eq!(
                tools.is_available("graphBuild"),
                surface == RuntimeSurface::Cli
            );
            assert_eq!(
                tools.is_available("graphQuery"),
                surface == RuntimeSurface::Cli
            );
        }
    }
}
