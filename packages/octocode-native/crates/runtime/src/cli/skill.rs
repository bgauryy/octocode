//! Native `octocode skill` — list/install/remove/check/info run without the
//! npm CLI (RFC post-audit-hardening-2026-09, R6). The canonical store
//! (`<octocode home>/skills/`) is the durable source of truth: the npm
//! launcher materializes bundled skills into it (and keeps the interactive
//! TTY picker); this command manages the store and the per-platform links
//! via the parity-gated `skill_install` engine. Unknown subcommands still
//! delegate to the npm CLI.

use octocode_native::runtime::ToolRuntime;
use octocode_native::skill_install::{
    self, InstallBundledSkillsOptions, SkillInstallMode, SkillInstallTarget, SkillPlatform,
    SkillScope,
};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

const NATIVE_SUBCOMMANDS: [&str; 5] = ["list", "install", "remove", "check", "info"];

/// A skill name must be a single path segment safe to `join` under the store.
/// Mirrors the npm registry guard (`^[A-Za-z0-9][A-Za-z0-9._-]*$`) so the
/// native path cannot escape the skill directory via `..`, `/`, or `\`.
fn valid_skill_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
}

struct Flags {
    names: Vec<String>,
    platform: Option<String>,
    add: Option<String>,
    project_dir: Option<String>,
    workspace: bool,
    mode: SkillInstallMode,
    force: bool,
    upgrade: bool,
    dry_run: bool,
    purge: bool,
    json: bool,
}

fn parse_flags(args: &[String]) -> Result<Flags, String> {
    let mut flags = Flags {
        names: Vec::new(),
        platform: None,
        add: None,
        project_dir: None,
        workspace: false,
        mode: SkillInstallMode::Symlink,
        force: false,
        upgrade: false,
        dry_run: false,
        purge: false,
        json: false,
    };
    let mut index = 0;
    while index < args.len() {
        let argument = args[index].as_str();
        let mut take_value = |name: &str| -> Result<String, String> {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| format!("{name} requires a value"))
        };
        match argument {
            "--platform" => flags.platform = Some(take_value("--platform")?),
            "--add" => flags.add = Some(take_value("--add")?),
            "--project-dir" => flags.project_dir = Some(take_value("--project-dir")?),
            "--mode" => {
                flags.mode = match take_value("--mode")?.as_str() {
                    "symlink" => SkillInstallMode::Symlink,
                    "copy" => SkillInstallMode::Copy,
                    "auto" => SkillInstallMode::Auto,
                    other => return Err(format!("Unknown --mode: {other}")),
                }
            }
            "--workspace" => flags.workspace = true,
            "--force" => flags.force = true,
            "--upgrade" => flags.upgrade = true,
            "--dry-run" => flags.dry_run = true,
            "--purge" => flags.purge = true,
            "--json" => flags.json = true,
            other if other.starts_with('-') => {
                return Err(format!("Unknown flag: {other}"));
            }
            name if valid_skill_name(name) => flags.names.push(name.to_owned()),
            name => {
                return Err(format!(
                    "Invalid skill name {name:?}: use only letters, digits, '.', '_', '-' and start with a letter or digit."
                ));
            }
        }
        index += 1;
    }
    Ok(flags)
}

fn os_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn platforms(flags: &Flags) -> Result<Vec<SkillPlatform>, String> {
    match flags.platform.as_deref() {
        Some(raw) => skill_install::parse_skill_platforms(raw),
        None => skill_install::parse_skill_platforms("all"),
    }
}

fn targets(flags: &Flags) -> Result<Vec<SkillInstallTarget>, String> {
    let home = os_home()
        .ok_or_else(|| "Cannot resolve the OS home directory (HOME/USERPROFILE)".to_owned())?;
    let mut targets: Vec<SkillInstallTarget> = platforms(flags)?
        .into_iter()
        .map(|platform| SkillInstallTarget {
            platform,
            scope: SkillScope::Global,
            home_dir: Some(home.to_string_lossy().into_owned()),
            project_dir: None,
        })
        .collect();
    if flags.workspace || flags.project_dir.is_some() {
        let project = flags.project_dir.clone().unwrap_or_else(|| {
            std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        targets.push(SkillInstallTarget {
            platform: octocode_native::skill_install::SkillPlatform::Codex,
            scope: SkillScope::Project,
            home_dir: None,
            project_dir: Some(project),
        });
    }
    Ok(targets)
}

fn frontmatter(skill_md: &Path) -> (Option<String>, Option<String>) {
    let Ok(text) = fs::read_to_string(skill_md) else {
        return (None, None);
    };
    let mut name = None;
    let mut description = None;
    let mut inside = false;
    for line in text.lines() {
        if line.trim() == "---" {
            if inside {
                break;
            }
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim().trim_matches(['"', '\'']).to_owned();
            match key.trim() {
                "name" => name = Some(value),
                "description" => description = Some(value),
                _ => {}
            }
        }
    }
    (name, description)
}

fn canonical_skills(canonical_dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Ok(entries) = fs::read_dir(canonical_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.join("SKILL.md").is_file() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort();
    names
}

fn location_status(path: &Path, canonical: &Path) -> Value {
    let meta = fs::symlink_metadata(path);
    let status = match &meta {
        Err(_) => "missing",
        Ok(meta) if meta.file_type().is_symlink() => {
            let target = fs::read_link(path).unwrap_or_default();
            let resolved = if target.is_absolute() {
                target
            } else {
                path.parent().unwrap_or(Path::new("/")).join(target)
            };
            if !resolved.exists() {
                "broken"
            } else if resolved
                .canonicalize()
                .ok()
                .zip(canonical.canonicalize().ok())
                .is_some_and(|(a, b)| a == b)
            {
                "linked"
            } else {
                "foreign-link"
            }
        }
        Ok(_) => {
            if skill_install::trees_match(path, canonical) {
                "installed"
            } else {
                "stale-copy"
            }
        }
    };
    json!({"path": path.to_string_lossy(), "status": status})
}

fn print_value(value: &Value, json_output: bool, render: impl Fn(&Value)) {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into())
        );
    } else {
        render(value);
    }
}

pub fn skill(runtime: &ToolRuntime, args: &[String]) -> u8 {
    let Some(subcommand) = args.first().map(String::as_str) else {
        eprintln!(
            "Usage: octocode skill <list|install|remove|check|info> [options]\n       (other subcommands delegate to the npm CLI)"
        );
        return 2;
    };
    if !NATIVE_SUBCOMMANDS.contains(&subcommand) {
        return super::system::skill(args);
    }
    let flags = match parse_flags(&args[1..]) {
        Ok(flags) => flags,
        Err(message) => {
            eprintln!("octocode skill {subcommand}: {message}");
            return 2;
        }
    };
    let canonical_dir = runtime.inspect_config().home.join("skills");
    match subcommand {
        "list" => list(&canonical_dir, &flags),
        "install" => install(&canonical_dir, &flags),
        "remove" => remove(&canonical_dir, &flags),
        "check" => check(&canonical_dir, &flags),
        "info" => info(&canonical_dir, &flags),
        _ => 2,
    }
}

fn list(canonical_dir: &Path, flags: &Flags) -> u8 {
    let skills: Vec<Value> = canonical_skills(canonical_dir)
        .into_iter()
        .map(|folder| {
            let dir = canonical_dir.join(&folder);
            let (name, description) = frontmatter(&dir.join("SKILL.md"));
            json!({
                "folder": folder,
                "name": name.unwrap_or_else(|| folder.clone()),
                "description": description.unwrap_or_default(),
                "path": dir.to_string_lossy(),
            })
        })
        .collect();
    let value = json!({
        "canonicalSkillsDir": canonical_dir.to_string_lossy(),
        "count": skills.len(),
        "skills": skills,
    });
    print_value(&value, flags.json, |value| {
        println!("Skills in {} :", canonical_dir.display());
        for skill in value["skills"].as_array().into_iter().flatten() {
            println!(
                "  {:<32} {}",
                skill["name"].as_str().unwrap_or_default(),
                skill["description"].as_str().unwrap_or_default()
            );
        }
        if value["count"] == 0 {
            println!(
                "  (none — materialize bundled skills with the npm CLI: npx -y octocode skill install --all)"
            );
        }
    });
    0
}

fn install(canonical_dir: &Path, flags: &Flags) -> u8 {
    let mut skills = Vec::new();
    if let Some(add) = &flags.add {
        let source = PathBuf::from(add);
        let (name, _) = frontmatter(&source.join("SKILL.md"));
        let Some(name) = name.or_else(|| {
            source
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        }) else {
            eprintln!("octocode skill install: cannot derive a skill name from --add {add}");
            return 2;
        };
        skills.push(skill_install::BundledSkill {
            name,
            source_path: source.to_string_lossy().into_owned(),
        });
    }
    for name in &flags.names {
        // Re-link an already-materialized canonical skill.
        skills.push(skill_install::BundledSkill {
            name: name.clone(),
            source_path: canonical_dir.join(name).to_string_lossy().into_owned(),
        });
    }
    if skills.is_empty() {
        eprintln!(
            "Usage: octocode skill install <name>... [--platform p,..] [--workspace] [--mode m] [--force] [--dry-run]\n       octocode skill install --add <path-to-skill-dir>"
        );
        return 2;
    }
    let targets = match targets(flags) {
        Ok(targets) => targets,
        Err(message) => {
            eprintln!("octocode skill install: {message}");
            return 2;
        }
    };
    let result = skill_install::install_bundled_skills(&InstallBundledSkillsOptions {
        skills,
        targets,
        canonical_skills_dir: Some(canonical_dir.to_string_lossy().into_owned()),
        mode: Some(flags.mode),
        force: Some(flags.force),
        upgrade: Some(flags.upgrade),
        dry_run: Some(flags.dry_run),
    });
    let ok = result.ok;
    let value = serde_json::to_value(&result).unwrap_or_else(|_| json!({}));
    print_value(&value, flags.json, |value| {
        for skill in value["skills"].as_array().into_iter().flatten() {
            println!(
                "{}: canonical {}",
                skill["name"].as_str().unwrap_or_default(),
                skill["canonicalStatus"].as_str().unwrap_or_default()
            );
            for destination in skill["destinations"].as_array().into_iter().flatten() {
                println!(
                    "  {:<10} {:<9} {}",
                    destination["platform"].as_str().unwrap_or_default(),
                    destination["status"].as_str().unwrap_or_default(),
                    destination["destination"].as_str().unwrap_or_default()
                );
            }
        }
        println!("summary: {}", value["summary"]);
    });
    if ok { 0 } else { 1 }
}

fn remove(canonical_dir: &Path, flags: &Flags) -> u8 {
    if flags.names.is_empty() {
        eprintln!("Usage: octocode skill remove <name>... [--platform p,..] [--purge] [--force]");
        return 2;
    }
    let targets = match targets(flags) {
        Ok(targets) => targets,
        Err(message) => {
            eprintln!("octocode skill remove: {message}");
            return 2;
        }
    };
    let mut report = Vec::new();
    let mut failed = false;
    for name in &flags.names {
        let canonical = canonical_dir.join(name);
        let mut removed = Vec::new();
        for target in &targets {
            let Ok(base) = skill_install::resolve_skill_destination(target) else {
                continue;
            };
            let destination = Path::new(&base).join(name);
            let Ok(meta) = fs::symlink_metadata(&destination) else {
                continue;
            };
            // Symlinks are always safe to unlink; a real directory is only
            // removed when it matches the canonical copy or with --force.
            let removable = meta.file_type().is_symlink()
                || flags.force
                || skill_install::trees_match(&destination, &canonical);
            if !removable {
                failed = true;
                removed.push(json!({
                    "path": destination.to_string_lossy(),
                    "status": "kept",
                    "reason": "directory content differs from the canonical copy; pass --force to delete",
                }));
                continue;
            }
            let outcome = if flags.dry_run {
                Ok(())
            } else if meta.file_type().is_symlink() {
                fs::remove_file(&destination)
            } else {
                fs::remove_dir_all(&destination)
            };
            removed.push(json!({
                "path": destination.to_string_lossy(),
                "status": if flags.dry_run {
                    "would-remove"
                } else if outcome.is_ok() {
                    "removed"
                } else {
                    "failed"
                },
            }));
            failed |= outcome.is_err();
        }
        if flags.purge && canonical.exists() {
            let outcome = if flags.dry_run {
                Ok(())
            } else {
                fs::remove_dir_all(&canonical)
            };
            removed.push(json!({
                "path": canonical.to_string_lossy(),
                "status": if flags.dry_run {
                    "would-remove"
                } else if outcome.is_ok() {
                    "removed"
                } else {
                    "failed"
                },
            }));
            failed |= outcome.is_err();
        }
        report.push(json!({"name": name, "locations": removed}));
    }
    let value = json!({"ok": !failed, "skills": report});
    print_value(&value, flags.json, |value| {
        for skill in value["skills"].as_array().into_iter().flatten() {
            println!("{}:", skill["name"].as_str().unwrap_or_default());
            for location in skill["locations"].as_array().into_iter().flatten() {
                println!(
                    "  {:<8} {}",
                    location["status"].as_str().unwrap_or_default(),
                    location["path"].as_str().unwrap_or_default()
                );
            }
        }
    });
    if failed { 1 } else { 0 }
}

fn check(canonical_dir: &Path, flags: &Flags) -> u8 {
    let names = if flags.names.is_empty() {
        canonical_skills(canonical_dir)
    } else {
        flags.names.clone()
    };
    let targets = match targets(flags) {
        Ok(targets) => targets,
        Err(message) => {
            eprintln!("octocode skill check: {message}");
            return 2;
        }
    };
    let mut healthy = true;
    let skills: Vec<Value> = names
        .iter()
        .map(|name| {
            let canonical = canonical_dir.join(name);
            let canonical_present = canonical.join("SKILL.md").is_file();
            let locations: Vec<Value> = targets
                .iter()
                .filter_map(|target| {
                    let base = skill_install::resolve_skill_destination(target).ok()?;
                    Some(location_status(&Path::new(&base).join(name), &canonical))
                })
                .collect();
            let broken = !canonical_present
                || locations.iter().any(|location| {
                    matches!(
                        location["status"].as_str(),
                        Some("broken") | Some("stale-copy")
                    )
                });
            healthy &= !broken;
            json!({
                "name": name,
                "canonical": {
                    "path": canonical.to_string_lossy(),
                    "present": canonical_present,
                },
                "status": if !canonical_present { "not-installed" } else if broken { "broken-or-stale" } else { "ok" },
                "locations": locations,
            })
        })
        .collect();
    let value = json!({"ok": healthy, "skills": skills});
    print_value(&value, flags.json, |value| {
        for skill in value["skills"].as_array().into_iter().flatten() {
            println!(
                "{} {}",
                if skill["status"] == "ok" {
                    "✓"
                } else {
                    "✗"
                },
                skill["name"].as_str().unwrap_or_default()
            );
            for location in skill["locations"].as_array().into_iter().flatten() {
                if location["status"] != "missing" {
                    println!(
                        "    {:<12} {}",
                        location["status"].as_str().unwrap_or_default(),
                        location["path"].as_str().unwrap_or_default()
                    );
                }
            }
        }
    });
    if healthy { 0 } else { 1 }
}

fn info(canonical_dir: &Path, flags: &Flags) -> u8 {
    let Some(name) = flags.names.first() else {
        eprintln!("Usage: octocode skill info <name>");
        return 2;
    };
    let skill_md = canonical_dir.join(name).join("SKILL.md");
    match fs::read_to_string(&skill_md) {
        Ok(text) => {
            if flags.json {
                let (skill_name, description) = frontmatter(&skill_md);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "name": skill_name.unwrap_or_else(|| (*name).clone()),
                        "description": description.unwrap_or_default(),
                        "path": skill_md.to_string_lossy(),
                        "content": text,
                    }))
                    .unwrap_or_else(|_| "{}".into())
                );
            } else {
                print!("{text}");
            }
            0
        }
        Err(_) => {
            eprintln!(
                "Skill not found in the canonical store: {}",
                skill_md.display()
            );
            eprintln!("Materialize bundled skills first: npx -y octocode skill install --all");
            3
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_flags, valid_skill_name};

    #[test]
    fn skill_names_reject_path_traversal_and_separators() {
        for good in ["octocode-research", "a", "a.b_c-1", "Skill2"] {
            assert!(valid_skill_name(good), "should accept {good:?}");
        }
        for bad in [
            "",
            "..",
            "../../../.ssh",
            "a/b",
            "a\\b",
            ".hidden",
            "-leading-dash",
            "with space",
            "n\0ul",
        ] {
            assert!(!valid_skill_name(bad), "should reject {bad:?}");
        }
    }

    #[test]
    fn parse_flags_rejects_a_traversal_name_before_any_filesystem_touch() {
        let args = vec![
            "../../../.ssh".to_owned(),
            "--force".to_owned(),
            "--purge".to_owned(),
        ];
        assert!(parse_flags(&args).is_err());
    }
}
