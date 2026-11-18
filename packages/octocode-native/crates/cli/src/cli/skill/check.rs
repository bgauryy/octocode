use super::{
    bundle::EmbeddedSkill,
    install::{self, SkillInstallTarget, SkillPlatform, SkillScope},
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Location {
    pub label: String,
    pub path: PathBuf,
    pub status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_target: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<&'static str>,
    #[serde(skip)]
    pub target: Option<SkillInstallTarget>,
    #[serde(skip)]
    pub managed: bool,
}

impl Location {
    pub(super) fn needs_repair(&self) -> bool {
        self.managed && (self.status == "broken" || self.content == Some("stale"))
    }
}

pub(super) struct Checked {
    pub home: Location,
    pub platforms: Vec<Location>,
    pub workspace: Location,
}

impl Checked {
    pub(super) fn locations(&self) -> impl Iterator<Item = &Location> {
        std::iter::once(&self.home)
            .chain(self.platforms.iter())
            .chain(std::iter::once(&self.workspace))
    }
    pub(super) fn status(&self) -> &'static str {
        if self.locations().any(|loc| loc.status == "broken") {
            "broken"
        } else if self.locations().any(|loc| loc.content == Some("stale")) {
            "stale"
        } else if self.locations().any(|loc| loc.status != "missing") {
            "ok"
        } else {
            "not-installed"
        }
    }
}

fn probe(
    label: &str,
    path: PathBuf,
    store: &Path,
    skill: &'static EmbeddedSkill,
    target: Option<SkillInstallTarget>,
    compared: &mut BTreeMap<PathBuf, Option<&'static str>>,
) -> Location {
    let link_target = fs::read_link(&path)
        .ok()
        .map(|target| install::normalize(&path.parent().unwrap_or(Path::new(".")).join(target)));
    let present = path.join("SKILL.md").is_file();
    let status = if link_target.is_some() {
        if present { "linked" } else { "broken" }
    } else if install::path_exists(&path) {
        if present { "installed" } else { "broken" }
    } else {
        "missing"
    };
    let store = store
        .canonicalize()
        .map(|path| install::normalize(&path))
        .unwrap_or_else(|_| install::normalize(store));
    let resolved = path
        .canonicalize()
        .ok()
        .map(|path| install::normalize(&path))
        .or_else(|| link_target.clone());
    let managed = link_target.is_none()
        || resolved
            .as_ref()
            .is_some_and(|resolved| resolved.starts_with(&store));
    let content = if present && managed {
        let key = resolved.unwrap_or_else(|| path.clone());
        *compared
            .entry(key)
            .or_insert_with(|| skill.freshness(&path))
    } else {
        None
    };
    Location {
        label: label.into(),
        path,
        status,
        link_target,
        content,
        target,
        managed,
    }
}

pub(super) fn inspect(
    skill: &'static EmbeddedSkill,
    store: &Path,
    platforms: &[SkillPlatform],
) -> Result<Checked, String> {
    let mut locations = Vec::new();
    let mut compared = BTreeMap::new();
    for platform in platforms {
        let target = super::target(*platform, None)?;
        let path = Path::new(&install::resolve_skill_destination(&target)?).join(skill.name);
        locations.push(probe(
            &super::platform_name(*platform),
            path,
            store,
            skill,
            Some(target),
            &mut compared,
        ));
    }
    let workspace_target = SkillInstallTarget {
        platform: SkillPlatform::Codex,
        scope: SkillScope::Project,
        home_dir: None,
        project_dir: Some(
            std::env::current_dir()
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .into_owned(),
        ),
    };
    let workspace_path =
        Path::new(&install::resolve_skill_destination(&workspace_target)?).join(skill.name);
    Ok(Checked {
        home: probe(
            "home",
            store.join(skill.name),
            store,
            skill,
            None,
            &mut compared,
        ),
        platforms: locations,
        workspace: probe(
            "workspace",
            workspace_path,
            store,
            skill,
            Some(workspace_target),
            &mut compared,
        ),
    })
}
