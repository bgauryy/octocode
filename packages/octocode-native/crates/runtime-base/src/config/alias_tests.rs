//! A renamed setting keeps its earlier names (`env.aliases` for its variable,
//! `keyAliases` for its `.octocoderc` key, both in the config contract): each
//! reads as the current name in every source, under the same trust rules, and
//! the current name wins. The security switch is the renamed setting.
use super::*;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn resolve(
    env: &[(&str, &str)],
    home_env: Option<&str>,
    workspace_env: Option<&str>,
) -> ConfigOutput {
    resolve_files(env, home_env, workspace_env, None, None)
}

fn resolve_files(
    env: &[(&str, &str)],
    home_env: Option<&str>,
    workspace_env: Option<&str>,
    home_rc: Option<&str>,
    workspace_rc: Option<&str>,
) -> ConfigOutput {
    let home = PathBuf::from("/synthetic/home");
    let file = |path: PathBuf, text: Option<&str>| match text {
        Some(text) => FileInput::Read {
            path,
            text: text.to_owned(),
        },
        None => FileInput::Missing { path },
    };
    let mut vars: BTreeMap<String, String> = env
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect();
    vars.insert("OCTOCODE_HOME".into(), "/synthetic/home/.octocode".into());
    resolve_config(&ConfigInput {
        env: vars,
        cwd: "/synthetic/cwd".into(),
        os_home: home.clone(),
        global_env: file(home.join(".env"), home_env),
        project_env: file("/synthetic/cwd/.octocode/.env".into(), workspace_env),
        config_file: file(home.join(".octocoderc"), home_rc),
        project_config_file: file("/synthetic/cwd/.octocode/.octocoderc".into(), workspace_rc),
        runtime_surface: RuntimeSurface::Mcp,
    })
}

/// Every alias the contract declares, with the binding it belongs to.
fn aliases() -> Vec<(&'static str, ConfigEnvBinding)> {
    CONFIG_FIELDS
        .iter()
        .filter_map(|field| field.env)
        .flat_map(|binding| binding.aliases.iter().map(move |alias| (*alias, binding)))
        .collect()
}

#[test]
fn an_alias_names_one_binding_and_no_current_variable() {
    let aliases = aliases();
    assert!(!aliases.is_empty(), "the contract declares no alias");
    for (alias, binding) in &aliases {
        assert_eq!(canonical_env_name(alias), binding.name, "{alias}");
        assert_eq!(canonical_env_name(binding.name), binding.name);
        assert!(
            !DECLARED_ENV_KEYS.contains(alias),
            "{alias} is also a current name"
        );
        assert_eq!(
            aliases.iter().filter(|(other, _)| other == alias).count(),
            1,
            "{alias}"
        );
    }
}

/// The effective environment holds current names only, whatever a source
/// spelled: validation, trust rules and the runtime's identity read one name.
#[test]
fn every_source_reads_an_alias_as_its_current_name() {
    for (alias, binding) in aliases() {
        // A value every kind of setting accepts or drops alone; only the
        // name it lands under is asserted.
        for output in [
            resolve(&[(alias, "x")], None, None),
            resolve(&[], Some(&format!("{alias}=x\n")), None),
        ] {
            assert_eq!(
                output.effective_env.get(binding.name).map(String::as_str),
                Some("x"),
                "{alias}"
            );
            assert!(!output.effective_env.contains_key(alias), "{alias}");
        }
    }
}

#[test]
fn the_current_name_wins_over_its_alias_in_one_source() {
    let switch = field::TOOLS_ENABLE_SECURITY.env.expect("bound");
    let alias = switch
        .aliases
        .first()
        .expect("the security switch has an alias");
    assert!(
        resolve(&[(alias, "true")], None, None)
            .resolved
            .tools
            .enable_security
    );
    assert!(
        !resolve(&[(alias, "true"), (switch.name, "false")], None, None)
            .resolved
            .tools
            .enable_security
    );
    assert!(
        resolve(&[(alias, "false"), (switch.name, "true")], None, None)
            .resolved
            .tools
            .enable_security
    );
    let both = format!("{alias}=true\n{}=false\n", switch.name);
    assert!(
        !resolve(&[], Some(&both), None)
            .resolved
            .tools
            .enable_security
    );
    // The process outranks a file under either spelling.
    assert!(
        !resolve(
            &[(switch.name, "false")],
            Some(&format!("{alias}=true\n")),
            None
        )
        .resolved
        .local
        .enabled
    );
}

/// A home-only switch stays home-only under its earlier name: a repository's
/// `.env` cannot turn it on, and the home file can.
#[test]
fn a_workspace_env_cannot_turn_on_a_home_only_setting_through_an_alias() {
    let switch = field::TOOLS_ENABLE_SECURITY.env.expect("bound");
    for name in switch.aliases.iter().chain([&switch.name]) {
        let line = format!("{name}=true\n");
        assert!(
            !resolve(&[], None, Some(&line))
                .resolved
                .tools
                .enable_security,
            "workspace {name}"
        );
        assert!(
            resolve(&[], Some(&line), None)
                .resolved
                .tools
                .enable_security,
            "home {name}"
        );
    }
}

/// The earlier key of a renamed setting in an `.octocoderc` reads as the
/// current one, which wins when a file holds both.
#[test]
fn an_earlier_config_key_reads_as_the_current_one() {
    let field = field::TOOLS_ENABLE_SECURITY;
    let earlier = field.key_aliases.first().expect("an earlier key");
    assert_ne!(*earlier, field.key);
    let rc = |key: &str, value: bool| format!("{{\"{}\":{{\"{key}\":{value}}}}}", field.section);
    let on = resolve_files(&[], None, None, Some(&rc(earlier, true)), None);
    assert!(on.resolved.tools.enable_security);
    assert!(on.diagnostics.is_empty(), "{:?}", on.diagnostics);
    let both = format!(
        "{{\"{}\":{{\"{earlier}\":true,\"{}\":false}}}}",
        field.section, field.key
    );
    assert!(
        !resolve_files(&[], None, None, Some(&both), None)
            .resolved
            .tools
            .enable_security
    );
    // Home-only under either key: a repository's file cannot turn it on.
    for key in [*earlier, field.key] {
        assert!(
            !resolve_files(&[], None, None, None, Some(&rc(key, true)))
                .resolved
                .tools
                .enable_security,
            "workspace {key}"
        );
    }
}

/// Every key alias names one field of its section and no current key there.
#[test]
fn a_key_alias_names_no_current_key_of_its_section() {
    for field in CONFIG_FIELDS
        .iter()
        .filter(|field| !field.key_aliases.is_empty())
    {
        for alias in field.key_aliases {
            assert!(
                !CONFIG_FIELDS
                    .iter()
                    .any(|other| other.section == field.section && other.key == *alias),
                "{}.{alias}",
                field.section
            );
        }
    }
}
