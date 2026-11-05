//! The `package` row: the public registry confirms a package, then the source
//! lists what it knows. Nothing reaches the source that the registry did not
//! confirm, and a failure at either step reads as not checked, never as clean.
use super::Net;
use super::answer;
use super::source::{Confirmed, SourceResult, Target, View, ask_all, into_row};
use super::upgrade;
use crate::id::ToolId;
use crate::providers::artifact::query::artifact_type;
use crate::providers::artifact::{
    ArtifactError, ArtifactItem, ArtifactProviderContext, execute_artifact,
};
use crate::response::paging::{self, PartialReason};
use crate::response::result::Continuation;
use octocode_contract_types::{ArtifactSearchQuery, ArtifactType, ErrorCode};
use serde_json::{Map, Value, json};

/// A package row's coordinates, wherever they came from.
pub(super) struct Package<'a> {
    pub kind: ArtifactType,
    pub name: &'a str,
    pub version: Option<&'a str>,
    /// The source's token: this row continues a walk the registry confirmed.
    pub resume: Option<&'a str>,
    /// `mainGoal` and `reasoning`, carried into the next page.
    pub brief: Map<String, Value>,
}

/// What the registry said beyond the target itself.
#[derive(Default)]
struct Notes {
    /// The requested version is not on the registry.
    absent: Option<ArtifactError>,
    prerelease_warning: Option<String>,
}

pub(super) async fn run(package: &Package<'_>, net: &Net<'_>) -> Result<Value, ArtifactError> {
    let (confirmed, notes) = match package.resume {
        Some(token) => (
            Confirmed {
                kind: package.kind,
                name: package.name.to_owned(),
                version: package.version.map(str::to_owned),
                resume: Some(token.to_owned()),
            },
            Notes::default(),
        ),
        None => confirm(package, net).await?,
    };
    let results = ask_all(&Target::Package(confirmed.clone()), net).await?;
    let upgrade = upgrade_for(&confirmed, &results, net).await;
    Ok(respond(package, &confirmed, notes, &results, &upgrade))
}

/// What the sources say to upgrade to.
#[derive(Debug, Default)]
struct Upgrade {
    /// The lowest version the sources say clears every listed advisory.
    to: Option<String>,
    /// Why none is named, or what OSV lists at that version besides.
    note: Option<String>,
}

/// The candidate from the listed fixes, then OSV's own answer at it. Another
/// advisory there moves the candidate past that one's fix, until OSV knows none
/// at it or no version can be named. A listed advisory still there names none.
async fn upgrade_for(confirmed: &Confirmed, results: &[SourceResult], net: &Net<'_>) -> Upgrade {
    let mut listed = results
        .iter()
        .flat_map(|result| &result.advisories)
        .cloned()
        .collect::<Vec<_>>();
    let finished = results.iter().all(|result| result.resume.is_none());
    let (Some(version), false, true) = (confirmed.version.as_deref(), listed.is_empty(), finished)
    else {
        return Upgrade::default();
    };
    let mut named = match upgrade::candidate(version, &listed) {
        Ok(candidate) => candidate,
        Err(why) => {
            return Upgrade {
                to: None,
                note: Some(format!("No upgrade named: {why}.")),
            };
        }
    };
    loop {
        let probe = Target::Package(Confirmed {
            kind: confirmed.kind,
            name: confirmed.name.clone(),
            version: Some(named.clone()),
            resume: None,
        });
        let Ok(answers) = ask_all(&probe, net).await else {
            return Upgrade {
                to: None,
                note: Some(format!(
                    "No upgrade named: {named} is the highest listed fix, but OSV could not confirm it."
                )),
            };
        };
        let at_named = answers
            .into_iter()
            .flat_map(|result| result.advisories)
            .collect::<Vec<_>>();
        if at_named.is_empty() {
            return Upgrade {
                to: Some(named),
                note: None,
            };
        }
        if let Some(still) = at_named
            .iter()
            .find(|row| listed.iter().any(|known| known.id == row.id))
        {
            return Upgrade {
                to: None,
                note: Some(format!(
                    "No upgrade named: OSV still lists {} at {named}, the highest listed fix.",
                    still.id
                )),
            };
        }
        // Each step goes strictly higher, and the call's budget bounds the walk.
        match upgrade::candidate(&named, &at_named) {
            Ok(next) => {
                listed.extend(at_named);
                named = next;
            }
            Err(_) => {
                let ids = at_named
                    .iter()
                    .map(|row| row.id.as_str())
                    .collect::<Vec<_>>();
                return Upgrade {
                    note: Some(format!(
                        "OSV lists other advisories at {named}: {}.",
                        ids.join(", ")
                    )),
                    to: Some(named),
                };
            }
        }
    }
}

/// The anonymous read of the public registry: the one resolve path, with no
/// authorization, no configured registry and no tag checks. A package it does
/// not know, or a failed read, ends the row here with nothing sent.
async fn confirm(
    package: &Package<'_>,
    net: &Net<'_>,
) -> Result<(Confirmed, Notes), ArtifactError> {
    let name = package.name;
    let kind = package.kind;
    let mut resolve = json!({
        "operation": "resolve",
        "ecosystem": artifact_type::as_str(kind),
        "packageName": name,
    });
    if let Some(version) = package.version {
        resolve["version"] = json!(version);
    }
    let resolve = crate::contracts::validate_query(ToolId::ArtifactSearch.as_str(), resolve)
        .map_err(|error| {
            ArtifactError::new(
                ErrorCode::InvalidInput,
                format!("{name} is not a {kind} package coordinate: {error}"),
            )
        })?;
    let resolve = serde_json::from_value::<ArtifactSearchQuery>(resolve)
        .map_err(|_| ArtifactError::new(ErrorCode::InvalidInput, "Invalid package query."))?;
    let context = ArtifactProviderContext {
        http: net.http,
        budget: net.budget,
        npm_registry: None,
        allow_private_registry: false,
        answers: net.answers,
        reused: net.reused,
        tags: None,
    };
    let page = match execute_artifact(&resolve, &context).await {
        Ok(page) => page,
        // The package is public and this version is not: the source may hear
        // the name, never a version the registry does not list.
        Err(error) if error.code == ErrorCode::VersionNotFound => {
            return Ok((
                Confirmed {
                    kind,
                    name: name.to_owned(),
                    version: None,
                    resume: None,
                },
                Notes {
                    absent: Some(error),
                    prerelease_warning: None,
                },
            ));
        }
        Err(mut error) => {
            error.message = format!(
                "{} Advisories were not checked; nothing was sent to OSV.",
                sentence(&error.message)
            );
            return Err(error);
        }
    };
    let Some(artifact) = page.artifacts.first() else {
        return Err(ArtifactError::new(
            ErrorCode::NotFound,
            format!(
                "{name} is not on the public {kind} registry, so it was not sent to OSV: advisories were not checked."
            ),
        )
        .with_status(404));
    };
    Ok((
        Confirmed {
            kind,
            name: source_name(artifact),
            version: artifact.version.clone(),
            resume: None,
        },
        Notes {
            absent: None,
            prerelease_warning: artifact.only_prerelease.then(|| {
                format!(
                    "{name}: the registry lists no stable release, so this checks its newest prerelease."
                )
            }),
        },
    ))
}

/// The name a source keys this package under: the registry's own spelling, and
/// for Go the module, since OSV keys Go by module, not by package path.
fn source_name(artifact: &ArtifactItem) -> String {
    artifact
        .module_path
        .clone()
        .unwrap_or_else(|| artifact.package_name.clone())
}

fn sentence(message: &str) -> String {
    let message = message.trim_end();
    if message.ends_with('.') {
        message.to_owned()
    } else {
        format!("{message}.")
    }
}

/// The next row of a stopped walk, or the row that reads the nearest published
/// versions, carrying the query's brief.
fn package_row(package: &Package<'_>, name: &str, version: Option<&str>) -> Map<String, Value> {
    let mut row = package.brief.clone();
    row.insert("operation".into(), json!("package"));
    row.insert(
        "ecosystem".into(),
        json!(artifact_type::as_str(package.kind)),
    );
    row.insert("packageName".into(), json!(name));
    if let Some(version) = version {
        row.insert("version".into(), json!(version));
    }
    row
}

/// The answer first, the target's facts, the sources' row, and what continues it.
fn respond(
    package: &Package<'_>,
    confirmed: &Confirmed,
    notes: Notes,
    results: &[SourceResult],
    upgrade: &Upgrade,
) -> Value {
    let name = confirmed.name.as_str();
    let version = confirmed.version.as_deref();
    let target = version.map_or_else(|| name.to_owned(), |version| format!("{name}@{version}"));
    let mut facts = answer::Facts::of(results);
    facts.upgrade_to = upgrade.to.as_deref();
    facts.upgrade_note = upgrade.note.as_deref();
    facts.version_evaluated = version.is_some();
    let mut data = answer::lead(&target, &facts);
    data["packageName"] = json!(name);
    match version {
        Some(version) => data["version"] = json!(version),
        None => data["versionEvaluated"] = json!(false),
    }
    let mut data = into_row(data, results, View::Brief);
    answer::read_whole(&mut data, results, &package.brief);
    paging::push_warnings(&mut data, notes.prerelease_warning);
    if let Some(absent) = notes.absent {
        paging::push_warnings(&mut data, [sentence(&absent.message)]);
        let nearest = absent
            .nearest
            .iter()
            .map(|near| Value::Object(package_row(package, name, Some(near))))
            .collect::<Vec<_>>();
        if !nearest.is_empty() {
            data["calls"]["repair"] =
                Continuation::related_input(ToolId::SecurityCheck, json!({ "queries": nearest }))
                    .build();
        }
    }
    if let Some(resume) = results.iter().find_map(|result| result.resume.as_ref()) {
        let mut next = package_row(package, name, version);
        next.insert("resume".into(), json!(resume.token));
        data["calls"]["nextPage"] =
            Continuation::continued(ToolId::SecurityCheck, Value::Object(next)).build();
        paging::mark_partial(
            &mut data,
            PartialReason::ProviderIncompleteResults,
            &[format!(
                "{} next.nextPage reads on from there.",
                resume.reason
            )],
        );
    }
    crate::response::result::finish_calls(&mut data);
    data
}

#[cfg(test)]
#[path = "package_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "package_upgrade_tests.rs"]
mod upgrade_tests;
