//! Tool availability from configuration and surface, refined by provider
//! readiness at host startup. Discovery and admission project the same
//! [`Availability`]; CLI help uses the configuration-only resolution.

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
    /// The configured provider failed its authenticated startup ping.
    ProviderUnavailable {
        code: octocode_contract_types::ErrorCode,
        message: String,
    },
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
            Self::ProviderUnavailable { .. } => Some("providerUnavailable"),
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
            Self::ProviderUnavailable { message, .. } => format!(
                "the classification provider ping failed: {message} Restart after checking the provider and credential."
            ),
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
            Self::Available | Self::Disabled | Self::CliOnly | Self::ProviderUnavailable { .. } => {
                Vec::new()
            }
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
#[derive(Clone, Debug)]
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

    pub(crate) fn with_provider_failure(
        &self,
        code: octocode_contract_types::ErrorCode,
        message: String,
    ) -> Self {
        let mut checked = self.clone();
        for (id, availability) in &mut checked.tools {
            if *id == ToolId::Clasify && availability.is_available() {
                *availability = Availability::ProviderUnavailable {
                    code,
                    message: message.clone(),
                };
            }
        }
        checked.available = checked
            .tools
            .iter()
            .filter(|(_, availability)| availability.is_available())
            .map(|(id, _)| id.as_str())
            .collect();
        checked
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

/// Settings a tool needs on every surface. Family comes from the contract; clone and
/// clasify add their own prerequisite. Clasify needs a credential;
/// the credential stays separate, so hiding public clasify never disables
/// goals on other tools.
fn prerequisites(id: ToolId) -> impl Iterator<Item = &'static ConfigFieldSpec> {
    [
        id.is_local().then_some(field::LOCAL_ENABLED),
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

/// The MCP-only restriction, shared by discovery, admission and emitted leads.
/// CLI callers retain access to beta tools without opting into MCP exposure.
pub(crate) fn mcp_restriction(config: &ConfigOutput, id: ToolId) -> Option<Availability> {
    if id.is_cli_only() {
        Some(Availability::CliOnly)
    } else if id.is_beta() && !config.resolved.local.beta {
        Some(Availability::MissingConfiguration(vec![field::LOCAL_BETA]))
    } else {
        None
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
    if surface != RuntimeSurface::Cli
        && let Some(restriction) = mcp_restriction(config, id)
    {
        return restriction;
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
mod tests;
