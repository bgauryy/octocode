use super::super::bundle::EmbeddedSkill;
use super::{
    ConcreteMode, SkillInstallMode, SkillPlatform, SkillScope, replace_directory, replace_with,
    same_tree,
};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in super::super) struct BundledSkill {
    pub(in super::super) name: String,
    pub(in super::super) source_path: String,
    #[serde(skip)]
    pub(in super::super) embedded: Option<&'static EmbeddedSkill>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in super::super) struct SkillInstallTarget {
    pub(in super::super) platform: SkillPlatform,
    pub(in super::super) scope: SkillScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in super::super) home_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in super::super) project_dir: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in super::super) struct InstallBundledSkillsOptions {
    pub(in super::super) skills: Vec<BundledSkill>,
    pub(in super::super) targets: Vec<SkillInstallTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(in super::super) canonical_skills_dir: Option<String>,
    #[serde(default)]
    pub(in super::super) mode: Option<SkillInstallMode>,
    #[serde(default)]
    pub(in super::super) force: Option<bool>,
    #[serde(default)]
    pub(in super::super) upgrade: Option<bool>,
    #[serde(default)]
    pub(in super::super) dry_run: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in super::super) enum CanonicalStatus {
    Installed,
    Upgraded,
    Unchanged,
    Conflict,
    Failed,
}

impl CanonicalStatus {
    /// The status as text: what happened, or with `dry_run` what would.
    pub(in super::super) fn text(self, dry_run: bool) -> &'static str {
        match (self, dry_run) {
            (Self::Installed, false) => "Installed",
            (Self::Installed, true) => "Would install",
            (Self::Upgraded, false) => "Upgraded",
            (Self::Upgraded, true) => "Would upgrade",
            (Self::Unchanged, _) => "Unchanged",
            (Self::Conflict, _) => "Conflict",
            (Self::Failed, _) => "Failed",
        }
    }
}

impl DestinationStatus {
    /// The status as text: what happened, or with `dry_run` what would.
    pub(in super::super) fn text(self, dry_run: bool) -> &'static str {
        match (self, dry_run) {
            (Self::Linked, false) => "Linked",
            (Self::Linked, true) => "Would link",
            (Self::Copied, false) => "Copied",
            (Self::Copied, true) => "Would copy",
            (Self::Unchanged, _) => "Unchanged",
            (Self::Conflict, _) => "Conflict",
            (Self::Failed, _) => "Failed",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in super::super) enum DestinationStatus {
    Linked,
    Copied,
    Unchanged,
    Conflict,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in super::super) struct SkillDestinationOutcome {
    pub(in super::super) platform: SkillPlatform,
    pub(in super::super) scope: SkillScope,
    pub(in super::super) destination: String,
    pub(in super::super) mode: ConcreteMode,
    pub(in super::super) status: DestinationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in super::super) link_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in super::super) error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in super::super) struct SkillInstallOutcome {
    pub(in super::super) name: String,
    pub(in super::super) source: String,
    pub(in super::super) canonical: String,
    pub(in super::super) canonical_status: CanonicalStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in super::super) canonical_error: Option<String>,
    pub(in super::super) destinations: Vec<SkillDestinationOutcome>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in super::super) struct SkillInstallSummary {
    pub(in super::super) installed: u32,
    pub(in super::super) upgraded: u32,
    pub(in super::super) linked: u32,
    pub(in super::super) copied: u32,
    pub(in super::super) unchanged: u32,
    pub(in super::super) conflicts: u32,
    pub(in super::super) failed: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in super::super) struct InstallBundledSkillsResult {
    pub(in super::super) ok: bool,
    pub(in super::super) action: &'static str,
    pub(in super::super) dry_run: bool,
    pub(in super::super) force: bool,
    pub(in super::super) upgrade: bool,
    pub(in super::super) canonical_skills_dir: String,
    pub(in super::super) skills: Vec<SkillInstallOutcome>,
    pub(in super::super) summary: SkillInstallSummary,
}

impl BundledSkill {
    pub(in super::super) fn matches(&self, destination: &Path) -> bool {
        match self.embedded {
            Some(skill) => skill.matches(destination),
            None => same_tree(Path::new(&self.source_path), destination),
        }
    }
    pub(in super::super) fn replace(&self, destination: &Path) -> Result<(), String> {
        match self.embedded {
            Some(skill) => replace_with(destination, |staged| skill.write(staged)),
            None => replace_directory(Path::new(&self.source_path), destination),
        }
    }
}
