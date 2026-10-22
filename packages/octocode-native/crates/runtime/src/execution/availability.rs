//! Tool availability, resolved once per runtime from its configuration and
//! surface. The catalog, admission, clasify context errors, and CLI help and
//! schema all project the same [`Availability`].

use crate::config::{ConfigFieldSpec, ConfigOutput, RuntimeSurface, field};
use crate::id::ToolId;
use std::sync::Arc;

/// Whether a tool runs on this runtime, and if not, the one reason.
#[derive(Clone, Debug, PartialEq)]
pub enum Availability {
    Available,
    /// Parked in this build (contract `disabled`): off on every surface,
    /// whatever the configuration.
    Disabled,
    /// `tools.disabled` lists it, or `tools.enabled` leaves it out.
    ToolList(&'static ConfigFieldSpec),
    /// A contract `cliOnly` tool on a surface other than the CLI.
    CliOnly,
    /// Settings it needs that are unset, in prerequisite order.
    MissingConfiguration(Vec<&'static ConfigFieldSpec>),
}

impl Availability {
    #[must_use]
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    /// The catalog's `unavailableReason`.
    #[must_use]
    pub(crate) fn reason_code(&self) -> Option<&'static str> {
        match self {
            Self::Available => None,
            Self::Disabled => Some("disabled"),
            Self::ToolList(_) => Some("toolsList"),
            Self::CliOnly => Some("cliOnly"),
            Self::MissingConfiguration(_) => Some("missingConfiguration"),
        }
    }

    /// What keeps the tool off, naming the setting (and its variable) to change.
    #[must_use]
    pub(crate) fn reason_text(&self) -> String {
        match self {
            Self::Available => "it is available.".into(),
            Self::Disabled => "it is disabled in this build.".into(),
            Self::ToolList(setting) if setting.path == field::TOOLS_DISABLED.path => {
                format!("{} lists it.", setting.display_name())
            }
            Self::ToolList(setting) => format!("{} does not list it.", setting.display_name()),
            Self::CliOnly => "only the CLI runs it.".into(),
            Self::MissingConfiguration(settings) => format!(
                "it needs {}.",
                settings
                    .iter()
                    .map(|setting| setting.display_name())
                    .collect::<Vec<_>>()
                    .join(" and ")
            ),
        }
    }

    /// The variables that would make the tool available: the tool list's
    /// variable, or every unset prerequisite's.
    #[must_use]
    pub fn env_vars(&self) -> Vec<&'static str> {
        match self {
            Self::Available | Self::Disabled | Self::CliOnly => Vec::new(),
            Self::ToolList(setting) => setting.env_name().into_iter().collect(),
            Self::MissingConfiguration(settings) => settings
                .iter()
                .filter_map(|setting| setting.env_name())
                .collect(),
        }
    }

    /// An unset beta gate or classification credential: the caller can opt in.
    #[must_use]
    pub(crate) fn needs_opt_in(&self) -> bool {
        matches!(self, Self::MissingConfiguration(settings) if settings.iter().any(|setting| {
            setting.path == field::LOCAL_BETA.path || setting.path == field::CLASSIFICATION_API.path
        }))
    }
}

/// Every tool's [`Availability`] on one runtime, in contract order.
#[derive(Debug)]
pub struct ToolAvailability {
    tools: Vec<(ToolId, Availability)>,
    available: Arc<[&'static str]>,
}

impl ToolAvailability {
    #[must_use]
    pub fn resolve(config: &ConfigOutput, surface: RuntimeSurface) -> Self {
        Self::resolve_with(config, surface, ToolId::is_disabled)
    }

    /// [`Self::resolve`] under an explicit `disabled` policy.
    fn resolve_with(
        config: &ConfigOutput,
        surface: RuntimeSurface,
        disabled: impl Fn(ToolId) -> bool,
    ) -> Self {
        let tools: Vec<_> = ToolId::ALL
            .into_iter()
            .map(|id| {
                let availability = if disabled(id) {
                    Availability::Disabled
                } else {
                    resolve(config, surface, id)
                };
                (id, availability)
            })
            .collect();
        let available = tools
            .iter()
            .filter(|(_, availability)| availability.is_available())
            .map(|(id, _)| id.as_str())
            .collect();
        Self { tools, available }
    }

    /// The availability of the tool named `tool`; `None` for an unknown name.
    #[must_use]
    pub fn get(&self, tool: &str) -> Option<&Availability> {
        self.tools
            .iter()
            .find(|(id, _)| id.as_str() == tool)
            .map(|(_, availability)| availability)
    }

    #[must_use]
    pub fn is_available(&self, tool: &str) -> bool {
        self.available.contains(&tool)
    }

    /// The names of the available tools, in contract order.
    #[must_use]
    pub fn available(&self) -> &Arc<[&'static str]> {
        &self.available
    }
}

/// The classification credential (`classification.api`): a blank value is
/// an explicit opt-out.
#[must_use]
pub fn classification_key(config: &ConfigOutput) -> Option<&str> {
    field::CLASSIFICATION_API
        .env_name()
        .and_then(|env| config.env_value(env))
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Settings a tool needs. Family and beta come from the contract; clone and
/// clasify add their own prerequisite. Clasify needs beta and a credential;
/// the credential stays separate, so hiding public clasify never disables
/// goals on other tools.
fn prerequisites(id: ToolId) -> impl Iterator<Item = &'static ConfigFieldSpec> {
    [
        id.is_local().then_some(field::LOCAL_ENABLED),
        id.is_beta().then_some(field::LOCAL_BETA),
        match id {
            ToolId::GhCloneRepo => Some(field::STORAGE_MODE),
            ToolId::Clasify => Some(field::CLASSIFICATION_API),
            _ => None,
        },
    ]
    .into_iter()
    .flatten()
}

fn is_met(config: &ConfigOutput, setting: &ConfigFieldSpec) -> bool {
    let resolved = &config.resolved;
    match setting.path {
        path if path == field::LOCAL_ENABLED.path => resolved.local.enabled,
        path if path == field::LOCAL_BETA.path => resolved.local.beta,
        path if path == field::STORAGE_MODE.path => {
            crate::config::is_persistent_storage_enabled(resolved)
        }
        path if path == field::CLASSIFICATION_API.path => classification_key(config).is_some(),
        _ => false,
    }
}

fn resolve(config: &ConfigOutput, surface: RuntimeSurface, id: ToolId) -> Availability {
    let tools = &config.resolved.tools;
    let listed = |names: &Option<Vec<String>>| {
        names
            .as_ref()
            .map(|names| names.iter().any(|name| name == id.as_str()))
    };
    if listed(&tools.disabled) == Some(true) {
        return Availability::ToolList(field::TOOLS_DISABLED);
    }
    if listed(&tools.enabled) == Some(false) {
        return Availability::ToolList(field::TOOLS_ENABLED);
    }
    if surface != RuntimeSurface::Cli && id.is_cli_only() {
        return Availability::CliOnly;
    }
    let missing: Vec<_> = prerequisites(id)
        .filter(|setting| !is_met(config, setting))
        .collect();
    if missing.is_empty() {
        Availability::Available
    } else {
        Availability::MissingConfiguration(missing)
    }
}

#[cfg(test)]
mod tests {
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
        // Only the unmet prerequisites are named: beta is set here.
        let beta = availability(&[("OCTOCODE_BETA", "true")], RuntimeSurface::Cli);
        let clasify = beta.get("clasify").expect("known");
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
                ("OCTOCODE_BETA", "true"),
                ("OCTOCODE_CLASSIFICATION_API", "k"),
            ],
            RuntimeSurface::Cli,
        );
        assert!(set.is_available("clasify"));
    }
}
