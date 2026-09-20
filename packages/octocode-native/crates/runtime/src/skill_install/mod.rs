//! Native port of `@octocodeai/octocode-skill-installer` (RFC
//! post-audit-hardening-2026-09, S7).
//!
//! Behavior parity with the TS implementation is enforced by
//! `parity_tests`, which replays the golden fixtures under
//! `packages/octocode-skill-installer/tests/fixtures/parity/` — the same
//! files the TS suite generates and asserts against. The TS path stays in
//! place until this suite is green in CI and the CLI delegates here.
//!
//! Windows note: destinations use `symlink_dir`; junction fallback is part
//! of the deferred Windows workstream (RFC unresolved question S12).

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

#[cfg(test)]
mod parity_tests;

pub const SKILL_NAME_PATTERN_HELP: &str =
    "letters, digits, '.', '_', '-'; must not start with a separator";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillPlatform {
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
pub enum SkillScope {
    Global,
    Project,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillInstallMode {
    #[default]
    Symlink,
    Copy,
    Auto,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConcreteMode {
    Symlink,
    Copy,
}

pub struct PlatformDescriptor {
    pub platform: SkillPlatform,
    pub aliases: &'static [&'static str],
    pub auto_mode: ConcreteMode,
    pub global_relative_path: &'static str,
    pub project_relative_path: &'static str,
}

/// Keep in byte-for-byte sync with `SKILL_PLATFORMS` in
/// `packages/octocode-skill-installer/src/index.ts` (the parity fixtures
/// exercise claude/cursor/codex; the registry itself is asserted by the
/// portable skill-sync contract test on the TS side).
pub const SKILL_PLATFORMS: [PlatformDescriptor; 7] = [
    PlatformDescriptor {
        platform: SkillPlatform::Pi,
        aliases: &[],
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".pi/agent/skills",
        project_relative_path: ".pi/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Cursor,
        aliases: &[],
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".cursor/skills",
        project_relative_path: ".cursor/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Claude,
        aliases: &["claude-desktop"],
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".claude/skills",
        project_relative_path: ".claude/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Codex,
        aliases: &["shared", "common", "agents", "codex-native"],
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".agents/skills",
        project_relative_path: ".agents/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Opencode,
        aliases: &[],
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".config/opencode/skills",
        project_relative_path: ".opencode/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Copilot,
        aliases: &[],
        auto_mode: ConcreteMode::Symlink,
        global_relative_path: ".copilot/skills",
        project_relative_path: ".github/skills",
    },
    PlatformDescriptor {
        platform: SkillPlatform::Gemini,
        aliases: &[],
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

pub fn parse_skill_platforms(raw: &str) -> Result<Vec<SkillPlatform>, String> {
    let mut platforms = Vec::new();
    for value in raw
        .split(',')
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        if value == "all" {
            return Ok(SKILL_PLATFORMS.iter().map(|entry| entry.platform).collect());
        }
        let matched = SKILL_PLATFORMS.iter().find(|entry| {
            serde_json::to_value(entry.platform)
                .ok()
                .and_then(|name| name.as_str().map(str::to_owned))
                == Some(value.clone())
                || entry.aliases.contains(&value.as_str())
        });
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

// ── Options and outcomes (serde shapes mirror the TS types) ───────────────

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BundledSkill {
    pub name: String,
    pub source_path: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallTarget {
    pub platform: SkillPlatform,
    pub scope: SkillScope,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallBundledSkillsOptions {
    pub skills: Vec<BundledSkill>,
    pub targets: Vec<SkillInstallTarget>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_skills_dir: Option<String>,
    #[serde(default)]
    pub mode: Option<SkillInstallMode>,
    #[serde(default)]
    pub force: Option<bool>,
    #[serde(default)]
    pub upgrade: Option<bool>,
    #[serde(default)]
    pub dry_run: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CanonicalStatus {
    Installed,
    Upgraded,
    Unchanged,
    Conflict,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DestinationStatus {
    Linked,
    Copied,
    Unchanged,
    Conflict,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDestinationOutcome {
    pub platform: SkillPlatform,
    pub scope: SkillScope,
    pub destination: String,
    pub mode: ConcreteMode,
    pub status: DestinationStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallOutcome {
    pub name: String,
    pub source: String,
    pub canonical: String,
    pub canonical_status: CanonicalStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical_error: Option<String>,
    pub destinations: Vec<SkillDestinationOutcome>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInstallSummary {
    pub installed: u32,
    pub upgraded: u32,
    pub linked: u32,
    pub copied: u32,
    pub unchanged: u32,
    pub conflicts: u32,
    pub failed: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallBundledSkillsResult {
    pub ok: bool,
    pub action: &'static str,
    pub dry_run: bool,
    pub force: bool,
    pub upgrade: bool,
    pub canonical_skills_dir: String,
    pub skills: Vec<SkillInstallOutcome>,
    pub summary: SkillInstallSummary,
}

// ── Path helpers (Node `path.resolve`/`path.join` lexical semantics) ──────

fn lexical_normalize(path: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    let absolute = path.starts_with('/');
    for component in Path::new(path).components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.last().is_some_and(|last| last != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..".into());
                }
            }
            Component::Normal(value) => parts.push(value.to_string_lossy().into_owned()),
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    let joined = parts.join("/");
    if absolute {
        format!("/{joined}")
    } else {
        joined
    }
}

fn node_join(a: &str, b: &str) -> String {
    lexical_normalize(&format!("{a}/{b}"))
}

fn node_resolve(path: &str) -> String {
    if path.starts_with('/') {
        lexical_normalize(path)
    } else {
        let cwd = std::env::current_dir().unwrap_or_default();
        lexical_normalize(&format!("{}/{path}", cwd.to_string_lossy()))
    }
}

fn path_exists(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

/// Public content comparison for freshness checks (`skill check`).
pub fn trees_match(left: &Path, right: &Path) -> bool {
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
            && fs::read(left).ok() == fs::read(right).ok();
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
        std::os::windows::fs::symlink_dir(&target, destination)?;
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

fn replace_directory(source: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "destination has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staged = parent.join(format!(
        ".octocode-skill-install-{}-{:x}",
        std::process::id(),
        destination.to_string_lossy().len()
    ));
    let backup = parent.join(format!(".octocode-skill-backup-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staged);
    let result = (|| -> Result<(), String> {
        copy_tree(source, &staged).map_err(|error| error.to_string())?;
        let had_previous = path_exists(destination);
        if had_previous {
            fs::rename(destination, &backup).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::rename(&staged, destination) {
            if had_previous {
                let _ = fs::rename(&backup, destination);
            }
            return Err(error.to_string());
        }
        if had_previous {
            let _ = fs::remove_dir_all(&backup);
        }
        Ok(())
    })();
    let _ = fs::remove_dir_all(&staged);
    result
}

fn replace_link(target: &Path, destination: &Path) -> Result<(), String> {
    let parent = destination
        .parent()
        .ok_or_else(|| "destination has no parent".to_owned())?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staged = parent.join(format!(
        ".octocode-skill-link-{}-{:x}",
        std::process::id(),
        destination.to_string_lossy().len()
    ));
    let backup = parent.join(format!(".octocode-skill-backup-{}", std::process::id()));
    let _ = fs::remove_file(&staged);
    let result = (|| -> Result<(), String> {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, &staged).map_err(|error| error.to_string())?;
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(target, &staged).map_err(|error| error.to_string())?;
        let had_previous = path_exists(destination);
        if had_previous {
            fs::rename(destination, &backup).map_err(|error| error.to_string())?;
        }
        if let Err(error) = fs::rename(&staged, destination) {
            if had_previous {
                let _ = fs::rename(&backup, destination);
            }
            return Err(error.to_string());
        }
        if had_previous {
            let _ = fs::remove_dir_all(&backup);
        }
        Ok(())
    })();
    let _ = fs::remove_file(&staged);
    result
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
    let resolved = if raw.starts_with('/') {
        lexical_normalize(&raw)
    } else {
        node_join(
            &link_path
                .parent()
                .unwrap_or(Path::new("/"))
                .to_string_lossy(),
            &raw,
        )
    };
    resolved == node_resolve(target_path)
}

fn effective_mode(mode: SkillInstallMode, platform: SkillPlatform) -> ConcreteMode {
    match mode {
        SkillInstallMode::Symlink => ConcreteMode::Symlink,
        SkillInstallMode::Copy => ConcreteMode::Copy,
        SkillInstallMode::Auto => descriptor(platform).auto_mode,
    }
}

fn validate_skill(name: &str, source: &str) -> Option<String> {
    let mut chars = name.chars();
    let head_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let tail_ok = name
        .chars()
        .skip(1)
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !head_ok || !tail_ok {
        return Some(format!("Invalid skill name: \"{name}\""));
    }
    let skill_file = PathBuf::from(node_join(source, "SKILL.md"));
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

pub fn resolve_skill_destination(target: &SkillInstallTarget) -> Result<String, String> {
    let home = node_resolve(target.home_dir.as_deref().unwrap_or_else(|| {
        // The homedir default is only used from real CLI runs.
        ""
    }));
    let root = match target.scope {
        SkillScope::Project => match target.project_dir.as_deref() {
            Some(dir) => node_resolve(dir),
            None => {
                return Err("projectDir is required for project skill installation".into());
            }
        },
        SkillScope::Global => home,
    };
    let entry = descriptor(target.platform);
    Ok(node_join(
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

pub fn install_bundled_skills(options: &InstallBundledSkillsOptions) -> InstallBundledSkillsResult {
    let dry_run = options.dry_run.unwrap_or(false);
    let force = options.force.unwrap_or(false);
    let upgrade = options.upgrade.unwrap_or(false);
    let mode = options.mode.unwrap_or_default();
    let canonical_skills_dir = node_resolve(
        options
            .canonical_skills_dir
            .as_deref()
            .unwrap_or(".octocode/skills"),
    );
    let mut summary = SkillInstallSummary::default();
    let mut outcomes = Vec::new();

    for skill in &options.skills {
        let source = node_resolve(&skill.source_path);
        let canonical = node_join(&canonical_skills_dir, &skill.name);
        let canonical_path = PathBuf::from(&canonical);
        let source_path = PathBuf::from(&source);
        let validation_error = validate_skill(&skill.name, &source);
        let canonical_exists = path_exists(&canonical_path);
        let canonical_differs = canonical_exists
            && validation_error.is_none()
            && !same_tree(&source_path, &canonical_path);
        let mut managed_copy_destinations = BTreeSet::new();
        if upgrade && canonical_differs {
            for target in &options.targets {
                if effective_mode(mode, target.platform) != ConcreteMode::Copy {
                    continue;
                }
                if let Ok(base) = resolve_skill_destination(target) {
                    let destination = node_join(&base, &skill.name);
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
        } else if same_tree(&source_path, &canonical_path) {
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
            match replace_directory(&source_path, &canonical_path) {
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
                    Ok(base) => node_join(&base, &skill.name),
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
                    && same_tree(&source_path, &destination_path);
                let unchanged = match target_mode {
                    ConcreteMode::Symlink => points_to(&destination_path, &canonical),
                    ConcreteMode::Copy => {
                        copy_already_matches_incoming
                            || (!managed_copy_upgrade
                                && same_tree(
                                    if dry_run && !path_exists(&canonical_path) {
                                        &source_path
                                    } else {
                                        &canonical_path
                                    },
                                    &destination_path,
                                ))
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
