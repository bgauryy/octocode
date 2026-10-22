use super::bundle::EmbeddedSkill;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[cfg(test)]
mod parity_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum SkillPlatform {
    Pi,
    Cursor,
    Claude,
    Codex,
    Opencode,
    Copilot,
    Gemini,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum SkillScope {
    Global,
    Project,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum SkillInstallMode {
    #[default]
    Symlink,
    Copy,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum ConcreteMode {
    Symlink,
    Copy,
}

pub(super) struct PlatformDescriptor {
    pub(super) platform: SkillPlatform,
    pub(super) auto_mode: ConcreteMode,
    pub(super) global_relative_path: &'static str,
    pub(super) project_relative_path: &'static str,
}

pub(super) const SKILL_PLATFORMS: [PlatformDescriptor; 7] = [
    PlatformDescriptor {
        platform: SkillPlatform::Pi,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".pi/agent/skills",
        project_relative_path: ".pi/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Cursor,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".cursor/skills",
        project_relative_path: ".cursor/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Claude,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".claude/skills",
        project_relative_path: ".claude/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Codex,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".agents/skills",
        project_relative_path: ".agents/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Opencode,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".config/opencode/skills",
        project_relative_path: ".opencode/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Copilot,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".copilot/skills",
        project_relative_path: ".github/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Gemini,
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".gemini/skills",
        project_relative_path: ".gemini/skills",
    },
];

fn descriptor(platform: SkillPlatform) -> &'static PlatformDescriptor {
    SKILL_PLATFORMS
        .iter()
        .find(|candidate| candidate.platform == platform)
        .unwrap_or(&SKILL_PLATFORMS[0])
}

pub(super) fn parse_skill_platforms(raw: &str) -> Result<Vec<SkillPlatform>, String> {
    let mut platforms = Vec::new();
    for value in raw
        .split(',')
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        if value == "all" {
            for entry in &SKILL_PLATFORMS {
                if !platforms.contains(&entry.platform) {
                    platforms.push(entry.platform);
                }
            }
            continue;
        }
        let matched = SKILL_PLATFORMS
            .iter()
            .find(|entry| entry.platform.name() == value);
        match matched {
            Some(entry) => {
                if !platforms.contains(&entry.platform) {
                    platforms.push(entry.platform);
                }
            }
            None => return Err(format!("Unknown platform: \"{value}\"")),
        }
    }
    Ok(platforms)
}

// Installation options and CLI reports; the fixtures pin their JSON shape.

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BundledSkill {
    pub(super) name: String,
    pub(super) source_path: String,
    #[serde(skip)]
    pub(super) embedded: Option<&'static EmbeddedSkill>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SkillInstallTarget {
    pub(super) platform: SkillPlatform,
    pub(super) scope: SkillScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) home_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) project_dir: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstallBundledSkillsOptions {
    pub(super) skills: Vec<BundledSkill>,
    pub(super) targets: Vec<SkillInstallTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) canonical_skills_dir: Option<String>,
    #[serde(default)]
    pub(super) mode: Option<SkillInstallMode>,
    #[serde(default)]
    pub(super) force: Option<bool>,
    #[serde(default)]
    pub(super) upgrade: Option<bool>,
    #[serde(default)]
    pub(super) dry_run: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum CanonicalStatus {
    Installed,
    Upgraded,
    Unchanged,
    Conflict,
    Failed,
}

impl CanonicalStatus {
    /// The status as text: what happened, or with `dry_run` what would.
    pub(super) fn text(self, dry_run: bool) -> &'static str {
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
    pub(super) fn text(self, dry_run: bool) -> &'static str {
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
pub(super) enum DestinationStatus {
    Linked,
    Copied,
    Unchanged,
    Conflict,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SkillDestinationOutcome {
    pub(super) platform: SkillPlatform,
    pub(super) scope: SkillScope,
    pub(super) destination: String,
    pub(super) mode: ConcreteMode,
    pub(super) status: DestinationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) link_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SkillInstallOutcome {
    pub(super) name: String,
    pub(super) source: String,
    pub(super) canonical: String,
    pub(super) canonical_status: CanonicalStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) canonical_error: Option<String>,
    pub(super) destinations: Vec<SkillDestinationOutcome>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SkillInstallSummary {
    pub(super) installed: u32,
    pub(super) upgraded: u32,
    pub(super) linked: u32,
    pub(super) copied: u32,
    pub(super) unchanged: u32,
    pub(super) conflicts: u32,
    pub(super) failed: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct InstallBundledSkillsResult {
    pub(super) ok: bool,
    pub(super) action: &'static str,
    pub(super) dry_run: bool,
    pub(super) force: bool,
    pub(super) upgrade: bool,
    pub(super) canonical_skills_dir: String,
    pub(super) skills: Vec<SkillInstallOutcome>,
    pub(super) summary: SkillInstallSummary,
}

// Absolute paths are normalized lexically, including missing dry-run targets.

pub(super) fn normalize(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut result = PathBuf::new();
    for component in absolute.components() {
        match component {
            #[cfg(windows)]
            Component::Prefix(prefix) => {
                use std::path::Prefix;
                match prefix.kind() {
                    Prefix::VerbatimDisk(drive) => result.push(format!("{}:", char::from(drive))),
                    Prefix::VerbatimUNC(server, share) => result.push(format!(
                        "\\\\{}\\{}",
                        server.to_string_lossy(),
                        share.to_string_lossy()
                    )),
                    _ => result.push(prefix.as_os_str()),
                }
            }
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            other => result.push(other.as_os_str()),
        }
    }
    result
}
fn join_path(a: &str, b: &str) -> String {
    normalize(&Path::new(a).join(b))
        .to_string_lossy()
        .into_owned()
}
fn resolve_path(path: &str) -> String {
    normalize(Path::new(path)).to_string_lossy().into_owned()
}

pub(super) fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Public content comparison for freshness checks (`skill check`).
pub(super) fn trees_match(left: &Path, right: &Path) -> bool {
    same_tree(left, right)
}

fn same_tree(left: &Path, right: &Path) -> bool {
    if !path_exists(left) || !path_exists(right) {
        return false;
    }
    let Ok(left_meta) = fs::symlink_metadata(left) else {
        return false;
    };
    let Ok(right_meta) = fs::symlink_metadata(right) else {
        return false;
    };
    if left_meta.file_type().is_symlink() || right_meta.file_type().is_symlink() {
        return left_meta.file_type().is_symlink()
            && right_meta.file_type().is_symlink()
            && fs::read_link(left).ok() == fs::read_link(right).ok();
    }
    if left_meta.is_file() || right_meta.is_file() {
        return left_meta.is_file()
            && right_meta.is_file()
            && fs::read(left)
                .ok()
                .zip(fs::read(right).ok())
                .is_some_and(|(a, b)| a == b);
    }
    if !left_meta.is_dir() || !right_meta.is_dir() {
        return false;
    }
    let list = |dir: &Path| -> Option<BTreeSet<String>> {
        let mut names = BTreeSet::new();
        for entry in fs::read_dir(dir).ok()? {
            names.insert(entry.ok()?.file_name().to_string_lossy().into_owned());
        }
        Some(names)
    };
    match (list(left), list(right)) {
        (Some(left_names), Some(right_names)) => {
            left_names == right_names
                && left_names
                    .iter()
                    .all(|name| same_tree(&left.join(name), &right.join(name)))
        }
        _ => false,
    }
}

fn copy_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    let meta = fs::symlink_metadata(source)?;
    if meta.file_type().is_symlink() {
        let target = fs::read_link(source)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, destination)?;
        #[cfg(windows)]
        if source.is_dir() {
            let absolute = source.parent().unwrap_or(Path::new(".")).join(&target);
            junction::create(normalize(&absolute), destination)?;
        } else {
            std::os::windows::fs::symlink_file(&target, destination)?;
        }
        return Ok(());
    }
    if meta.is_dir() {
        fs::create_dir_all(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
        return Ok(());
    }
    fs::copy(source, destination).map(|_| ())
}

pub(super) fn remove_path(path: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    if junction::exists(path)? {
        return junction::delete(path);
    }
    let meta = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    if meta.file_type().is_symlink() && path.is_dir() {
        return fs::remove_dir(path);
    }
    if meta.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

fn replace_with(
    destination: &Path,
    prepare: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<(), String> {
    let parent = destination.parent().ok_or("destination has no parent")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let transaction = tempfile::Builder::new()
        .prefix(".octocode-skill-")
        .tempdir_in(parent)
        .map_err(|error| error.to_string())?;
    let staged = transaction.path().join("next");
    let backup = transaction.path().join("previous");
    prepare(&staged).map_err(|error| error.to_string())?;
    let previous = path_exists(destination);
    if previous {
        fs::rename(destination, &backup).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&staged, destination) {
        if previous && let Err(rollback) = fs::rename(&backup, destination) {
            let saved = transaction.keep();
            return Err(format!(
                "{error}; rollback failed: {rollback}; previous contents saved in {}",
                saved.display()
            ));
        }
        return Err(error.to_string());
    }
    if previous {
        remove_path(&backup).map_err(|error| {
            format!(
                "Installed, but could not remove backup {}: {error}",
                transaction.keep().display()
            )
        })?;
    }
    Ok(())
}

fn replace_directory(source: &Path, destination: &Path) -> Result<(), String> {
    if physical_destination(destination).starts_with(physical_destination(source)) {
        return Err("The installation destination must not be inside its source".into());
    }
    replace_with(destination, |staged| copy_tree(source, staged))
}

fn physical_destination(path: &Path) -> PathBuf {
    let absolute = normalize(path);
    let mut current = absolute.as_path();
    let mut missing = Vec::new();
    loop {
        if let Ok(mut resolved) = current.canonicalize() {
            for component in missing.into_iter().rev() {
                resolved.push(component);
            }
            return resolved;
        }
        let (Some(parent), Some(name)) = (current.parent(), current.file_name()) else {
            return absolute;
        };
        missing.push(name.to_owned());
        current = parent;
    }
}
fn replace_link(target: &Path, destination: &Path) -> Result<(), String> {
    replace_with(destination, |staged| {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, staged)
        }
        #[cfg(windows)]
        {
            junction::create(target, staged)
        }
    })
}

fn points_to(link_path: &Path, target_path: &str) -> bool {
    let Ok(meta) = fs::symlink_metadata(link_path) else {
        return false;
    };
    if !meta.file_type().is_symlink() {
        return false;
    }
    let Ok(raw) = fs::read_link(link_path) else {
        return false;
    };
    let raw = raw.to_string_lossy().into_owned();
    let resolved = if Path::new(&raw).is_absolute() {
        resolve_path(&raw)
    } else {
        join_path(
            &link_path
                .parent()
                .unwrap_or(Path::new("/"))
                .to_string_lossy(),
            &raw,
        )
    };
    resolved == resolve_path(target_path)
}

fn effective_mode(mode: SkillInstallMode, platform: SkillPlatform) -> ConcreteMode {
    match mode {
        SkillInstallMode::Symlink => ConcreteMode::Symlink,
        SkillInstallMode::Copy => ConcreteMode::Copy,
        SkillInstallMode::Auto => descriptor(platform).auto_mode,
    }
}

fn validate_skill(name: &str, source: &str) -> Option<String> {
    if !super::bundle::valid_name(name) {
        return Some(format!("Invalid skill name: \"{name}\""));
    }
    let skill_file = PathBuf::from(join_path(source, "SKILL.md"));
    if !skill_file.exists() {
        return Some(format!(
            "Bundled skill is missing: {}",
            skill_file.display()
        ));
    }
    match fs::symlink_metadata(&skill_file) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => None,
        _ => Some(format!(
            "Bundled SKILL.md must be a regular file: {}",
            skill_file.display()
        )),
    }
}

pub(super) fn resolve_skill_destination(target: &SkillInstallTarget) -> Result<String, String> {
    let root = match target.scope {
        SkillScope::Project => match target.project_dir.as_deref() {
            Some(dir) => resolve_path(dir),
            None => {
                return Err("projectDir is required for project skill installation".into());
            }
        },
        SkillScope::Global => resolve_path(
            target
                .home_dir
                .as_deref()
                .ok_or("homeDir is required for global skill installation")?,
        ),
    };
    let entry = descriptor(target.platform);
    Ok(join_path(
        &root,
        match target.scope {
            SkillScope::Global => entry.global_relative_path,
            SkillScope::Project => entry.project_relative_path,
        },
    ))
}

fn increment_canonical(summary: &mut SkillInstallSummary, status: CanonicalStatus) {
    match status {
        CanonicalStatus::Installed => summary.installed += 1,
        CanonicalStatus::Upgraded => summary.upgraded += 1,
        CanonicalStatus::Unchanged => summary.unchanged += 1,
        CanonicalStatus::Conflict => summary.conflicts += 1,
        CanonicalStatus::Failed => summary.failed += 1,
    }
}

fn increment_destination(summary: &mut SkillInstallSummary, status: DestinationStatus) {
    match status {
        DestinationStatus::Linked => summary.linked += 1,
        DestinationStatus::Copied => summary.copied += 1,
        DestinationStatus::Unchanged => summary.unchanged += 1,
        DestinationStatus::Conflict => summary.conflicts += 1,
        DestinationStatus::Failed => summary.failed += 1,
    }
}

pub(super) fn install_bundled_skills(
    options: &InstallBundledSkillsOptions,
) -> InstallBundledSkillsResult {
    let dry_run = options.dry_run.unwrap_or(false);
    let force = options.force.unwrap_or(false);
    let upgrade = options.upgrade.unwrap_or(false);
    let mode = options.mode.unwrap_or_default();
    let canonical_skills_dir = resolve_path(
        options
            .canonical_skills_dir
            .as_deref()
            .unwrap_or(".octocode/skills"),
    );
    let mut summary = SkillInstallSummary::default();
    let mut outcomes = Vec::new();

    for skill in &options.skills {
        let source = if skill.embedded.is_some() {
            skill.source_path.clone()
        } else {
            resolve_path(&skill.source_path)
        };
        let canonical = join_path(&canonical_skills_dir, &skill.name);
        let canonical_path = PathBuf::from(&canonical);
        let validation_error = if skill.embedded.is_some() {
            None
        } else {
            validate_skill(&skill.name, &source)
        }
        .or_else(|| {
            let source = physical_destination(Path::new(&source));
            let destination = physical_destination(&canonical_path);
            (skill.embedded.is_none() && source != destination && destination.starts_with(source))
                .then(|| "The installation destination must not be inside its source".into())
        });
        let canonical_exists = path_exists(&canonical_path);
        let canonical_differs =
            canonical_exists && validation_error.is_none() && !skill.matches(&canonical_path);
        let mut managed_copy_destinations = BTreeSet::new();
        if upgrade && canonical_differs {
            for target in &options.targets {
                if effective_mode(mode, target.platform) != ConcreteMode::Copy {
                    continue;
                }
                if let Ok(base) = resolve_skill_destination(target) {
                    let destination = join_path(&base, &skill.name);
                    if same_tree(&canonical_path, Path::new(&destination)) {
                        managed_copy_destinations.insert(destination);
                    }
                }
            }
        }

        let mut canonical_error = None;
        let canonical_status = if let Some(error) = validation_error {
            canonical_error = Some(error);
            CanonicalStatus::Failed
        } else if skill.matches(&canonical_path) {
            CanonicalStatus::Unchanged
        } else if canonical_exists && !force && !upgrade {
            CanonicalStatus::Conflict
        } else if dry_run {
            if canonical_exists {
                CanonicalStatus::Upgraded
            } else {
                CanonicalStatus::Installed
            }
        } else {
            match skill.replace(&canonical_path) {
                Ok(()) => {
                    if canonical_exists {
                        CanonicalStatus::Upgraded
                    } else {
                        CanonicalStatus::Installed
                    }
                }
                Err(error) => {
                    canonical_error = Some(error);
                    CanonicalStatus::Failed
                }
            }
        };
        increment_canonical(&mut summary, canonical_status);

        let mut destinations = Vec::new();
        let mut seen = BTreeSet::new();
        if matches!(
            canonical_status,
            CanonicalStatus::Installed | CanonicalStatus::Upgraded | CanonicalStatus::Unchanged
        ) {
            for target in &options.targets {
                let target_mode = effective_mode(mode, target.platform);
                let destination = match resolve_skill_destination(target) {
                    Ok(base) => join_path(&base, &skill.name),
                    Err(error) => {
                        let failed = SkillDestinationOutcome {
                            platform: target.platform,
                            scope: target.scope,
                            destination: String::new(),
                            mode: target_mode,
                            status: DestinationStatus::Failed,
                            link_target: None,
                            error: Some(error),
                        };
                        increment_destination(&mut summary, failed.status);
                        destinations.push(failed);
                        continue;
                    }
                };
                if !seen.insert(destination.clone()) {
                    continue;
                }
                let destination_path = PathBuf::from(&destination);
                let exists = path_exists(&destination_path);
                let managed_copy_upgrade = upgrade
                    && canonical_status == CanonicalStatus::Upgraded
                    && target_mode == ConcreteMode::Copy
                    && managed_copy_destinations.contains(&destination);
                let copy_already_matches_incoming = upgrade
                    && canonical_status == CanonicalStatus::Upgraded
                    && target_mode == ConcreteMode::Copy
                    && skill.matches(&destination_path);
                let unchanged = physical_destination(&destination_path)
                    == physical_destination(&canonical_path)
                    || match target_mode {
                        ConcreteMode::Symlink => points_to(&destination_path, &canonical),
                        ConcreteMode::Copy => {
                            copy_already_matches_incoming
                                || (!managed_copy_upgrade
                                    && if dry_run {
                                        skill.matches(&destination_path)
                                    } else {
                                        same_tree(&canonical_path, &destination_path)
                                    })
                        }
                    };
                let mut error = None;
                let status = if unchanged {
                    DestinationStatus::Unchanged
                } else if exists && !force && !managed_copy_upgrade {
                    DestinationStatus::Conflict
                } else if dry_run {
                    match target_mode {
                        ConcreteMode::Symlink => DestinationStatus::Linked,
                        ConcreteMode::Copy => DestinationStatus::Copied,
                    }
                } else {
                    let attempt = match target_mode {
                        ConcreteMode::Symlink => replace_link(&canonical_path, &destination_path)
                            .map(|()| DestinationStatus::Linked),
                        ConcreteMode::Copy => replace_directory(&canonical_path, &destination_path)
                            .map(|()| DestinationStatus::Copied),
                    };
                    match attempt {
                        Ok(status) => status,
                        Err(message) => {
                            error = Some(message);
                            DestinationStatus::Failed
                        }
                    }
                };
                increment_destination(&mut summary, status);
                destinations.push(SkillDestinationOutcome {
                    platform: target.platform,
                    scope: target.scope,
                    destination,
                    mode: target_mode,
                    status,
                    link_target: (target_mode == ConcreteMode::Symlink).then(|| canonical.clone()),
                    error,
                });
            }
        }

        outcomes.push(SkillInstallOutcome {
            name: skill.name.clone(),
            source,
            canonical,
            canonical_status,
            canonical_error,
            destinations,
        });
    }

    InstallBundledSkillsResult {
        ok: summary.conflicts == 0 && summary.failed == 0,
        action: if dry_run {
            "dry-run"
        } else if upgrade {
            "upgrade"
        } else {
            "install"
        },
        dry_run,
        force,
        upgrade,
        canonical_skills_dir,
        skills: outcomes,
        summary,
    }
}

impl BundledSkill {
    pub(super) fn matches(&self, destination: &Path) -> bool {
        match self.embedded {
            Some(skill) => skill.matches(destination),
            None => same_tree(Path::new(&self.source_path), destination),
        }
    }
    fn replace(&self, destination: &Path) -> Result<(), String> {
        match self.embedded {
            Some(skill) => replace_with(destination, |staged| skill.write(staged)),
            None => replace_directory(Path::new(&self.source_path), destination),
        }
    }
}

impl SkillPlatform {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Pi => "pi",
            Self::Cursor => "cursor",
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
            Self::Copilot => "copilot",
            Self::Gemini => "gemini",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_publication_restores_previous_content_and_cleans_staging() {
        let root = tempfile::tempdir().expect("root");
        let destination = root.path().join("skill");
        fs::create_dir(&destination).expect("directory");
        fs::write(destination.join("SKILL.md"), "previous").expect("write");
        // No staged path forces the publish rename to fail after the backup exists.
        assert!(replace_with(&destination, |_| Ok(())).is_err());
        assert_eq!(
            fs::read_to_string(destination.join("SKILL.md")).expect("restored"),
            "previous"
        );
        assert_eq!(fs::read_dir(root.path()).expect("list").count(), 1);
    }

    #[test]
    fn platform_parser_deduplicates_and_validates_every_token() {
        assert_eq!(
            parse_skill_platforms(" Claude,claude,CODEX ").expect("platforms"),
            vec![SkillPlatform::Claude, SkillPlatform::Codex]
        );
        assert_eq!(
            parse_skill_platforms("codex,all").expect("all").len(),
            SKILL_PLATFORMS.len()
        );
        for invalid in [
            "all,typo",
            "agents",
            "shared",
            "claude-desktop",
            "codex-native",
        ] {
            assert!(parse_skill_platforms(invalid).is_err(), "{invalid}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_skill_manifest_is_rejected_before_install() {
        let root = tempfile::tempdir().expect("root");
        fs::write(root.path().join("actual.md"), "instructions").expect("write");
        std::os::unix::fs::symlink("actual.md", root.path().join("SKILL.md")).expect("link");
        assert!(
            validate_skill("valid", &root.path().to_string_lossy())
                .is_some_and(|error| error.contains("regular file"))
        );
    }
}
