mod args;
mod bundle;
mod check;
mod environment;
mod install;
mod maintain;
use maintain::{check, remove};

use super::ToolRuntime;
pub(crate) use args::SkillCommand;
use args::{InstallArgs, Mode};
use bundle::{EmbeddedSkill, SKILLS};
use install::{
    BundledSkill, InstallBundledSkillsOptions, SkillInstallMode, SkillInstallTarget, SkillPlatform,
    SkillScope,
};
use octocode_contract_types::ErrorCode;
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

type Reply = Result<(Value, String, u8), String>;

pub(super) async fn run(runtime: &ToolRuntime, command: SkillCommand, json_output: bool) -> u8 {
    let store = runtime.config().home.join("skills");
    let needs_env = matches!(&command, SkillCommand::List | SkillCommand::Info { .. })
        || matches!(&command, SkillCommand::Check(args) if !args.no_env);
    let github_auth = needs_env && super::system::active_token_source(runtime).await.is_some();
    let result = match command {
        SkillCommand::Install(args) => install(&store, args),
        SkillCommand::Remove(args) => remove(&store, args),
        SkillCommand::List => list(runtime, &store, github_auth),
        SkillCommand::Info { name } => info(runtime, &name, github_auth),
        SkillCommand::Check(args) => check(runtime, &store, args, github_auth),
    };
    match result {
        Ok((value, text, exit)) => {
            let written = if json_output {
                super::write_json(&value)
            } else {
                super::write_text(&text)
            };
            if written == 0 { exit } else { written }
        }
        Err(error) => {
            super::emit_error(ErrorCode::InvalidInput, &error, json_output);
            2
        }
    }
}

fn platform_name(platform: SkillPlatform) -> String {
    platform.name().into()
}
fn platforms(raw: Option<&str>) -> Result<Vec<SkillPlatform>, String> {
    let platforms = install::parse_skill_platforms(raw.unwrap_or("all"))?;
    if platforms.is_empty() {
        return Err("--platform must name at least one platform".into());
    }
    Ok(platforms)
}
fn target(platform: SkillPlatform, project: Option<&Path>) -> Result<SkillInstallTarget, String> {
    Ok(SkillInstallTarget {
        platform,
        scope: if project.is_some() {
            SkillScope::Project
        } else {
            SkillScope::Global
        },
        home_dir: if project.is_none() {
            Some(
                octocode_io::env::home_dir()
                    .ok_or("Cannot resolve the user home directory")?
                    .to_string_lossy()
                    .into_owned(),
            )
        } else {
            None
        },
        project_dir: project.map(|path| install::normalize(path).to_string_lossy().into_owned()),
    })
}
fn bundled(name: &str) -> Result<&'static EmbeddedSkill, String> {
    SKILLS
        .iter()
        .find(|skill| skill.name == name)
        .ok_or_else(|| format!("Unknown bundled skill: {name}. Run octocode skill list."))
}
fn source(skill: &'static EmbeddedSkill) -> BundledSkill {
    BundledSkill {
        name: skill.name.into(),
        source_path: format!("bundled:{}", skill.name),
        embedded: Some(skill),
    }
}
fn names(names: Vec<String>) -> Result<Vec<String>, String> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for name in names {
        if !bundle::valid_name(&name) {
            return Err(format!("Invalid skill name: {name:?}"));
        }
        if seen.insert(name.clone()) {
            result.push(name);
        }
    }
    Ok(result)
}

fn list(runtime: &ToolRuntime, store: &Path, github_auth: bool) -> Reply {
    let platforms = platforms(None)?;
    let mut skills = Vec::new();
    let mut text = String::new();
    let mut installed = 0;
    for skill in SKILLS {
        let checked = check::inspect(skill, store, &platforms)?;
        let status = checked.status();
        if status != "not-installed" {
            installed += 1;
        }
        let description = skill.description;
        text.push_str(&format!(
            "{}  [{}]\n  {}\n",
            skill.name, status, description
        ));
        skills.push(json!({"name":skill.name,"folder":skill.name,"description":description,"status":status,
            "linkedPlatforms":checked.platforms.iter().filter(|location|matches!(location.status,"installed"|"linked")).map(|location|&location.label).collect::<Vec<_>>(),
            "hasWorkspaceLink":matches!(checked.workspace.status,"installed"|"linked"),
            "env":environment::status(runtime,skill.name,github_auth)}));
    }
    Ok((
        json!({"success":true,"source":"bundled","count":skills.len(),"installedCount":installed,"skills":skills}),
        text,
        0,
    ))
}

fn info(runtime: &ToolRuntime, name: &str, github_auth: bool) -> Reply {
    let skill = bundled(name)?;
    Ok((
        json!({"success":true,"skill":{"name":name,"folder":name,"description":skill.description,
        "dir":format!("bundled:{name}"),"skillMd":skill.text(),"env":environment::status(runtime,name,github_auth)}}),
        skill.text().into(),
        0,
    ))
}

fn install(store: &Path, args: InstallArgs) -> Reply {
    let selected = names(args.names)?;
    let skills = if let Some(path) = args.add {
        if selected.len() > 1 {
            return Err("--add accepts at most one skill name override".into());
        }
        let directory = if path.file_name().is_some_and(|name| name == "SKILL.md") {
            path.parent()
                .ok_or("SKILL.md needs a parent directory")?
                .to_owned()
        } else {
            path
        };
        let directory = directory
            .canonicalize()
            .map_err(|error| format!("Cannot open {}: {error}", directory.display()))?;
        let manifest = directory.join("SKILL.md");
        if !fs::symlink_metadata(&manifest).is_ok_and(|metadata| metadata.is_file()) {
            return Err(format!(
                "SKILL.md must be a regular file: {}",
                manifest.display()
            ));
        }
        let name = match selected.first() {
            Some(name) => name.clone(),
            None => {
                let text = fs::read_to_string(manifest).map_err(|error| error.to_string())?;
                let declared = bundle::declared_name(&text)?;
                if directory
                    .file_name()
                    .is_none_or(|folder| folder != declared.as_str())
                {
                    return Err("The SKILL.md name must match its folder; provide a name override to rename it".into());
                }
                declared
            }
        };
        vec![BundledSkill {
            name,
            source_path: directory.to_string_lossy().into_owned(),
            embedded: None,
        }]
    } else if args.all {
        SKILLS.iter().map(source).collect()
    } else {
        if selected.is_empty() {
            return Err("Choose skill names, --all, or --add PATH".into());
        }
        selected
            .iter()
            .map(|name| bundled(name).map(source))
            .collect::<Result<Vec<_>, _>>()?
    };
    if let Some(project) = &args.project_dir
        && !project.is_dir()
    {
        return Err(format!(
            "Project directory does not exist: {}",
            project.display()
        ));
    }
    let targets = match &args.platform {
        Some(raw) => platforms(Some(raw))?
            .into_iter()
            .map(|platform| target(platform, args.project_dir.as_deref()))
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    let canonical = args.path.as_deref().unwrap_or(store);
    let result = install::install_bundled_skills(&InstallBundledSkillsOptions {
        skills,
        targets,
        canonical_skills_dir: Some(canonical.to_string_lossy().into_owned()),
        mode: Some(match args.mode {
            Mode::Symlink => SkillInstallMode::Symlink,
            Mode::Copy => SkillInstallMode::Copy,
            Mode::Auto => SkillInstallMode::Auto,
        }),
        force: Some(args.force),
        upgrade: Some(args.upgrade),
        dry_run: Some(args.dry_run),
    });
    let mut text = String::new();
    for skill in &result.skills {
        text.push_str(&format!(
            "{}: {} {}\n",
            skill.name,
            skill.canonical_status.text(args.dry_run),
            skill.canonical
        ));
        if let Some(error) = &skill.canonical_error {
            text.push_str(&format!("  {error}\n"));
        }
        for location in &skill.destinations {
            text.push_str(&format!(
                "  {}: {} {}\n",
                platform_name(location.platform),
                location.status.text(args.dry_run),
                location.destination
            ));
            if let Some(error) = &location.error {
                text.push_str(&format!("    {error}\n"));
            }
        }
    }
    if args.dry_run {
        text.push_str("Dry run: no files changed.\n");
    }
    let exit = u8::from(!result.ok);
    let mut value = serde_json::to_value(result).map_err(|error| error.to_string())?;
    if args.platform.is_none() && args.path.is_none() {
        let hint = "Skills are stored in the Octocode home. To expose them to an agent, add --platform <name> --global or --project-dir <dir>.";
        value["warnings"] = json!([hint]);
        text.push_str(hint);
    }
    Ok((value, text, exit))
}
