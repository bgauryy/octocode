use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

mod outcome;
#[cfg(test)]
mod parity_tests;
mod platforms;
mod tree;
pub(super) use outcome::{
    BundledSkill, CanonicalStatus, DestinationStatus, InstallBundledSkillsOptions,
    InstallBundledSkillsResult, SkillDestinationOutcome, SkillInstallOutcome, SkillInstallSummary,
    SkillInstallTarget,
};
use platforms::descriptor;
pub(super) use platforms::{
    ConcreteMode, SkillInstallMode, SkillPlatform, SkillScope, parse_skill_platforms,
};
use tree::{
    join_path, physical_destination, points_to, replace_directory, replace_link, replace_with,
    resolve_path,
};
pub(super) use tree::{normalize, path_exists, remove_path, same_tree};

// Installation options and CLI reports; the fixtures pin their JSON shape.

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

/// What the call asks of every skill.
#[derive(Clone, Copy)]
struct Run {
    dry_run: bool,
    force: bool,
    upgrade: bool,
    mode: SkillInstallMode,
}

pub(super) fn install_bundled_skills(
    options: &InstallBundledSkillsOptions,
) -> InstallBundledSkillsResult {
    let run = Run {
        dry_run: options.dry_run.unwrap_or(false),
        force: options.force.unwrap_or(false),
        upgrade: options.upgrade.unwrap_or(false),
        mode: options.mode.unwrap_or_default(),
    };
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
        let managed = if run.upgrade && canonical_differs {
            managed_copies(run, &options.targets, &skill.name, &canonical_path)
        } else {
            BTreeSet::new()
        };
        let (canonical_status, canonical_error) = install_canonical(
            run,
            skill,
            &canonical_path,
            canonical_exists,
            validation_error,
        );
        increment_canonical(&mut summary, canonical_status);

        let mut destinations = Vec::new();
        let mut seen = BTreeSet::new();
        if matches!(
            canonical_status,
            CanonicalStatus::Installed | CanonicalStatus::Upgraded | CanonicalStatus::Unchanged
        ) {
            for target in &options.targets {
                let destination = match resolve_skill_destination(target) {
                    Ok(base) => join_path(&base, &skill.name),
                    Err(error) => {
                        let failed = SkillDestinationOutcome {
                            platform: target.platform,
                            scope: target.scope,
                            destination: String::new(),
                            mode: effective_mode(run.mode, target.platform),
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
                let outcome = install_destination(
                    run,
                    skill,
                    target,
                    destination,
                    (&canonical, canonical_status),
                    &managed,
                );
                increment_destination(&mut summary, outcome.status);
                destinations.push(outcome);
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
        action: if run.dry_run {
            "dry-run"
        } else if run.upgrade {
            "upgrade"
        } else {
            "install"
        },
        dry_run: run.dry_run,
        force: run.force,
        upgrade: run.upgrade,
        canonical_skills_dir,
        skills: outcomes,
        summary,
    }
}

/// The copy-mode destinations of skill `name` that hold the current
/// canonical tree: an upgrade replaces them as managed copies.
fn managed_copies(
    run: Run,
    targets: &[SkillInstallTarget],
    name: &str,
    canonical_path: &Path,
) -> BTreeSet<String> {
    let mut managed = BTreeSet::new();
    for target in targets {
        if effective_mode(run.mode, target.platform) != ConcreteMode::Copy {
            continue;
        }
        if let Ok(base) = resolve_skill_destination(target) {
            let destination = join_path(&base, name);
            if same_tree(canonical_path, Path::new(&destination)) {
                managed.insert(destination);
            }
        }
    }
    managed
}

/// Install or upgrade the canonical copy of `skill` (a validation error
/// fails it), and the error that failed it.
fn install_canonical(
    run: Run,
    skill: &BundledSkill,
    canonical_path: &Path,
    canonical_exists: bool,
    validation_error: Option<String>,
) -> (CanonicalStatus, Option<String>) {
    if let Some(error) = validation_error {
        return (CanonicalStatus::Failed, Some(error));
    }
    let written = if canonical_exists {
        CanonicalStatus::Upgraded
    } else {
        CanonicalStatus::Installed
    };
    if skill.matches(canonical_path) {
        (CanonicalStatus::Unchanged, None)
    } else if canonical_exists && !run.force && !run.upgrade {
        (CanonicalStatus::Conflict, None)
    } else if run.dry_run {
        (written, None)
    } else {
        match skill.replace(canonical_path) {
            Ok(()) => (written, None),
            Err(error) => (CanonicalStatus::Failed, Some(error)),
        }
    }
}

/// Link or copy the canonical copy of `skill` to one target's
/// `destination`. `managed` holds the copies an upgrade may replace.
fn install_destination(
    run: Run,
    skill: &BundledSkill,
    target: &SkillInstallTarget,
    destination: String,
    (canonical, canonical_status): (&str, CanonicalStatus),
    managed: &BTreeSet<String>,
) -> SkillDestinationOutcome {
    let target_mode = effective_mode(run.mode, target.platform);
    let canonical_path = Path::new(canonical);
    let destination_path = PathBuf::from(&destination);
    let exists = path_exists(&destination_path);
    let upgraded_copy = run.upgrade
        && canonical_status == CanonicalStatus::Upgraded
        && target_mode == ConcreteMode::Copy;
    let managed_copy_upgrade = upgraded_copy && managed.contains(&destination);
    let copy_already_matches_incoming = upgraded_copy && skill.matches(&destination_path);
    let unchanged = physical_destination(&destination_path) == physical_destination(canonical_path)
        || match target_mode {
            ConcreteMode::Symlink => points_to(&destination_path, canonical),
            ConcreteMode::Copy => {
                copy_already_matches_incoming
                    || (!managed_copy_upgrade
                        && if run.dry_run {
                            skill.matches(&destination_path)
                        } else {
                            same_tree(canonical_path, &destination_path)
                        })
            }
        };
    let mut error = None;
    let status = if unchanged {
        DestinationStatus::Unchanged
    } else if exists && !run.force && !managed_copy_upgrade {
        DestinationStatus::Conflict
    } else if run.dry_run {
        match target_mode {
            ConcreteMode::Symlink => DestinationStatus::Linked,
            ConcreteMode::Copy => DestinationStatus::Copied,
        }
    } else {
        let attempt = match target_mode {
            ConcreteMode::Symlink => {
                replace_link(canonical_path, &destination_path).map(|()| DestinationStatus::Linked)
            }
            ConcreteMode::Copy => replace_directory(canonical_path, &destination_path)
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
    SkillDestinationOutcome {
        platform: target.platform,
        scope: target.scope,
        destination,
        mode: target_mode,
        status,
        link_target: (target_mode == ConcreteMode::Symlink).then(|| canonical.to_owned()),
        error,
    }
}

#[cfg(test)]
mod tests;
