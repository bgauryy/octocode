use super::super::ToolRuntime;
use super::args::{CheckArgs, RemoveArgs};
use super::bundle::SKILLS;
use super::install::{InstallBundledSkillsOptions, SkillInstallMode};
use super::{Reply, bundled, install, names, platform_name, platforms, source, target};
use serde_json::json;
use std::{collections::BTreeSet, fs, path::Path};

pub(super) fn check(
    runtime: &ToolRuntime,
    store: &Path,
    args: CheckArgs,
    github_auth: bool,
) -> Reply {
    let selected = names(args.names)?;
    let skills = if selected.is_empty() {
        SKILLS.iter().collect::<Vec<_>>()
    } else {
        selected
            .iter()
            .map(|name| bundled(name))
            .collect::<Result<Vec<_>, _>>()?
    };
    let platforms = platforms(args.platform.as_deref())?;
    let mut reports = Vec::new();
    let mut failures = Vec::new();
    let mut text = String::new();
    for skill in skills {
        let mut checked = super::check::inspect(skill, store, &platforms)?;
        if args.fix {
            let repairs = checked
                .locations()
                .filter(|location| {
                    location.needs_repair() && (location.label != "workspace" || args.workspace)
                })
                .collect::<Vec<_>>();
            for location in repairs {
                let result = install::install_bundled_skills(&InstallBundledSkillsOptions {
                    skills: vec![source(skill)],
                    targets: location.target.clone().into_iter().collect(),
                    canonical_skills_dir: Some(store.to_string_lossy().into_owned()),
                    mode: Some(if location.link_target.is_some() {
                        SkillInstallMode::Symlink
                    } else {
                        SkillInstallMode::Copy
                    }),
                    force: Some(true),
                    upgrade: Some(true),
                    dry_run: Some(args.dry_run),
                });
                if !result.ok {
                    failures.push(serde_json::to_value(result).map_err(|error| error.to_string())?);
                }
            }
            if !args.dry_run {
                checked = super::check::inspect(skill, store, &platforms)?;
            }
        }
        let status = checked.status();
        let env = if args.no_env {
            json!({"readiness":"ok","params":[],"hint":""})
        } else {
            super::environment::status(runtime, skill.name, github_auth)
        };
        text.push_str(&format!(
            "{}: {status}{}\n",
            skill.name,
            if args.no_env {
                String::new()
            } else {
                format!(" · env {}", env["readiness"].as_str().unwrap_or("ok"))
            }
        ));
        if let Some(hint) = env["hint"].as_str().filter(|hint| !hint.is_empty()) {
            text.push_str(&format!("  {hint}\n"));
        }
        for location in checked
            .locations()
            .filter(|location| location.status != "missing")
        {
            text.push_str(&format!(
                "  {}: {}{} {}\n",
                location.label,
                location.status,
                location
                    .content
                    .map(|content| format!(" ({content})"))
                    .unwrap_or_default(),
                location.path.display()
            ));
        }
        reports.push(json!({"name":skill.name,"installStatus":status,"home":checked.home,"platforms":checked.platforms,"workspace":checked.workspace,"env":env}));
    }
    let count = |status: &str| {
        reports
            .iter()
            .filter(|report| report["installStatus"] == status)
            .count()
    };
    let env_count = |status: &str| {
        reports
            .iter()
            .filter(|report| report["env"]["readiness"] == status)
            .count()
    };
    let success = count("broken") == 0 && count("stale") == 0 && failures.is_empty();
    if !success {
        text.push_str("Repair existing managed installations with octocode skill check --fix.\n");
    }
    if args.dry_run {
        text.push_str("Dry run: no files changed.\n");
    }
    if !failures.is_empty() {
        text.push_str(&format!("Repair failures: {}\n", json!(failures)));
    }
    Ok((
        json!({"success":success,"summary":{"install":{"ok":count("ok"),"broken":count("broken"),"stale":count("stale"),"notInstalled":count("not-installed"),"total":reports.len()},
        "env":{"ready":env_count("ready"),"partial":env_count("partial"),"needsConfig":env_count("needs-config"),"noParamsNeeded":env_count("ok")}},"skills":reports,"repairFailures":failures}),
        text,
        u8::from(!success),
    ))
}

pub(super) fn remove(store: &Path, args: RemoveArgs) -> Reply {
    let selected = if args.all {
        let mut selected = BTreeSet::new();
        selected.extend(SKILLS.iter().map(|skill| skill.name.to_owned()));
        if let Ok(entries) = fs::read_dir(store) {
            selected.extend(
                entries
                    .filter_map(Result::ok)
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .filter(|name| super::bundle::valid_name(name)),
            );
        }
        selected.into_iter().collect()
    } else {
        names(args.names)?
    };
    if selected.is_empty() {
        return Err("Choose skill names or --all".into());
    }
    let platforms = platforms(args.platform.as_deref())?;
    let mut reports = Vec::new();
    let (mut removed, mut skipped, mut failed) = (0, 0, 0);
    let mut text = String::new();
    for name in selected {
        let canonical = store.join(&name);
        let mut locations = Vec::new();
        for platform in &platforms {
            let target = target(*platform, None)?;
            locations.push((
                platform_name(*platform),
                Path::new(&install::resolve_skill_destination(&target)?).join(&name),
            ));
        }
        if args.platform.is_none() {
            locations.push((
                "workspace".into(),
                std::env::current_dir()
                    .map_err(|error| error.to_string())?
                    .join(".agents/skills")
                    .join(&name),
            ));
            // Keep the canonical bytes until every platform copy has been compared.
            locations.push(("home".into(), canonical.clone()));
        }
        let mut seen = BTreeSet::new();
        let mut outcomes = Vec::new();
        let mut found = false;
        for (label, path) in locations {
            if !seen.insert(path.clone()) {
                continue;
            }
            if !install::path_exists(&path) {
                skipped += 1;
                outcomes.push(json!({"target":label,"path":path,"status":"missing"}));
                continue;
            }
            found = true;
            let safe = label == "home"
                || fs::read_link(&path).is_ok()
                || args.force
                || install::trees_match(&path, &canonical)
                || bundled(&name).is_ok_and(|skill| skill.matches(&path));
            let error = if !safe {
                Some("Edited or unrecognized directory; pass --force to remove it".to_owned())
            } else if args.dry_run {
                None
            } else {
                install::remove_path(&path)
                    .err()
                    .map(|error| error.to_string())
            };
            let status = if error.is_some() {
                failed += 1;
                "failed"
            } else {
                removed += 1;
                if args.dry_run {
                    "would-remove"
                } else {
                    "removed"
                }
            };
            text.push_str(&format!("{name}: {status} {}\n", path.display()));
            if let Some(error) = &error {
                text.push_str(&format!("  {error}\n"));
            }
            let mut outcome = json!({"target":label,"path":path,"status":status});
            if let Some(error) = error {
                outcome["error"] = json!(error);
            }
            outcomes.push(outcome);
        }
        reports.push(json!({"name":name,"nothingFound":!found,"targets":outcomes}));
    }
    if args.dry_run {
        text.push_str("Dry run: no files changed.\n");
    }
    Ok((
        json!({"success":failed==0,"dryRun":args.dry_run,"skills":reports,"summary":{"removed":removed,"skipped":skipped,"failed":failed}}),
        text,
        u8::from(failed != 0),
    ))
}
