//! The `package` row: the public registry confirms a package, then the source
//! lists what it knows. Nothing reaches the source that the registry did not
//! confirm, and a failure at either step reads as not checked, never as clean.
use super::Net;
use super::answer;
use super::dispute;
use super::ecosystem::{self, Ecosystem};
use super::global;
use super::record::SecurityAdvisory;
use super::source::{Confirmed, SourceResult, Target, View, ask_all, into_row, names_release};
use super::summary;
use super::trust::{self, Registered, Trust};
use super::update::{self, Latest};
use super::upgrade;
use crate::id::ToolId;
use crate::providers::artifact::query::artifact_type;
use crate::providers::artifact::{
    ArtifactError, ArtifactItem, ArtifactProviderContext, Pin, exact_pins, execute_artifact,
};
use crate::response::paging::{self, PartialReason};
use crate::response::result::Continuation;
use octocode_contract_types::{ArtifactSearchQuery, ErrorCode, SecurityCheckView};
use serde_json::{Map, Value, json};

/// A package row's coordinates, wherever they came from.
pub(super) struct Package<'a> {
    pub ecosystem: Ecosystem,
    pub name: &'a str,
    pub version: Option<&'a str>,
    /// A distribution release; refused on any other ecosystem.
    pub release: Option<&'a str>,
    /// The source's token: this row continues a walk the registry confirmed.
    pub resume: Option<&'a str>,
    /// `mainGoal` and `reasoning`, carried into the next page.
    pub brief: Map<String, Value>,
    /// How much of the finished row the answer states.
    pub view: SecurityCheckView,
}

/// What the registry said beyond the target itself.
#[derive(Default)]
struct Notes {
    /// The requested version is not on the registry.
    absent: Option<ArtifactError>,
    prerelease_warning: Option<String>,
    /// Runtime dependencies the confirmed version pins to one exact version:
    /// the advisories filed against them are not this package's.
    pinned: Vec<Pin>,
    /// No registry reads this ecosystem: the name and version went to OSV as given.
    unconfirmed: bool,
    /// What the registry states of the confirmed version, for the trust read.
    registered: Option<Registered>,
    /// The confirmed version's release date and the registry's latest stable.
    current_published_at: Option<String>,
    latest: Option<Latest>,
    /// The registry still to ask for its latest stable release: the confirmed
    /// version was pinned, so it says nothing of the latest.
    latest_lookup: Option<octocode_contract_types::ArtifactType>,
    /// Why the latest release is unknown, when the registry read failed.
    latest_unread: Option<Unread>,
    /// The trust read, once made.
    trust: Option<Trust>,
    /// Why the last page of a resumed walk could not read the source
    /// repository's advisories: the registry read that names it failed.
    repository_unread: Option<Unread>,
    /// A source lists the package in the release asked, so a version with no
    /// record there is a finding and not an unnamed release.
    release_named: bool,
}

/// A registry read that failed once the version was confirmed: the row names
/// the registry as a failed source and is partial, never silently complete.
struct Unread {
    code: ErrorCode,
    why: String,
}

/// Adds the failed registry to `sources[]` and marks the row partial.
fn registry_failed(data: &mut Value, unread: &Unread) {
    let source = json!({
        "source": "registry", "status": "failed",
        "errorCode": unread.code, "why": unread.why,
    });
    match data["sources"].as_array_mut() {
        Some(sources) => sources.push(source),
        None => data["sources"] = json!([source]),
    }
    super::source::source_failed(data, std::slice::from_ref(&unread.why));
}

/// The row's target: confirmed by the registry, or, for a resumed walk, the
/// one its first page confirmed.
async fn target_of(
    package: &Package<'_>,
    net: &Net<'_>,
) -> Result<(Confirmed, Notes), ArtifactError> {
    match package.resume {
        Some(token) => Ok((
            Confirmed {
                ecosystem: package.ecosystem,
                name: package.name.to_owned(),
                version: package.version.map(str::to_owned),
                resume: Some(token.to_owned()),
                release: package.release.map(str::to_owned),
                import: None,
            },
            Notes {
                unconfirmed: ecosystem::registry(package.ecosystem)?.is_none(),
                ..Notes::default()
            },
        )),
        None => confirm(package, net).await,
    }
}

pub(super) async fn run(package: &Package<'_>, net: &Net<'_>) -> Result<Value, ArtifactError> {
    ecosystem::check_release(package.ecosystem, package.release)?;
    let (confirmed, mut notes) = target_of(package, net).await?;
    let resumed = package.resume.is_some();
    // GitHub's read of the source repository and the registry's latest release
    // do not depend on OSV's answer.
    let registered = notes.registered.take();
    let target = Target::Package(confirmed.clone());
    let lookup = notes.latest_lookup.take();
    let (results, trust, latest) = tokio::join!(
        ask_all(&target, net),
        async {
            match registered {
                Some(registered) => Some(trust::read(registered, &confirmed, net).await),
                None => None,
            }
        },
        async {
            match lookup {
                Some(kind) => Some(read_latest(package, kind, net).await),
                None => None,
            }
        }
    );
    // OSV did not answer the first page: the registry's facts and GitHub's,
    // read beside it, still stand, as the advisory operation keeps GitHub's
    // record. A resumed page has none of its own, so it stays an error.
    let (results, osv_failed) = match results {
        Ok(results) => (results, None),
        Err(error) if !resumed => (Vec::new(), Some(error)),
        Err(error) => return Err(error),
    };
    notes.trust = trust;
    match latest {
        Some(Ok(latest)) => notes.latest = latest,
        Some(Err(error)) => {
            // A version whose repository GitHub lacks is followed to the one
            // its latest release names: unread, that one is not compared either.
            let unread_too = notes
                .trust
                .as_ref()
                .is_some_and(trust::Trust::repository_missing);
            notes.latest_unread = Some(latest_unread(
                package,
                &error,
                if unread_too {
                    "whether it is outdated is unknown, and the advisories of the repository it names were not compared with OSV's records"
                } else {
                    "whether it is outdated is unknown"
                },
            ));
        }
        None => {}
    }
    // A version whose repository GitHub lacks is read where its latest
    // release says the project lives.
    if let (Some(trust), Some(latest)) = (&mut notes.trust, &notes.latest) {
        trust.follow(latest, &confirmed, net).await;
    }
    // A failed by-name ask leaves the empty answer not covered, and says why.
    let probe_failed = match release_named(&confirmed, &results, net).await {
        // Not asked again: OSV just failed.
        _ if osv_failed.is_some() => None,
        Ok(named) => {
            notes.release_named = named;
            None
        }
        Err(error) => Some(error.message),
    };
    let finished = results.iter().all(|result| result.resume.is_none());
    // Each of these reads OSV's answer and none reads another's: the upgrade
    // probes, the aliases set against the list, GitHub's database (compared
    // with OSV's whole list only) and, on the last page of a resumed walk, the
    // source repository (the first page could not compare its advisories with
    // a walk that was not whole; this page holds all of OSV's records).
    let (upgrade, disputes, mut github, ()) = tokio::join!(
        async {
            match osv_failed {
                // No upgrade stands on OSV's list when OSV did not answer.
                Some(_) => Upgrade::default(),
                None => upgrade_for(&confirmed, &results, net).await,
            }
        },
        disputes(package.view, &confirmed, &results, net),
        async {
            match finished {
                true => global::read(&confirmed, net).await,
                false => None,
            }
        },
        async {
            if resumed && finished {
                last_page_trust(package, &confirmed, &mut notes, net).await;
            }
        },
    );
    let mut results = results;
    let disputes = disputes.mark(&mut results);
    if let Some(trust) = &mut notes.trust {
        match osv_failed {
            None => trust.merge(&mut results, finished),
            // Nothing of OSV's to compare the repository's advisories with.
            Some(_) => trust.osv_unanswered(),
        }
    }
    if let Some(github) = &mut github {
        match osv_failed {
            None => github.merge(&mut results),
            // Nothing of OSV's to merge with or set against: GitHub's records
            // are the rows, GitHub's word.
            Some(_) => github.stand_alone(&mut results),
        }
    }
    if let Some(version) = confirmed.version.as_deref() {
        for row in results.iter_mut().flat_map(|result| &mut result.advisories) {
            upgrade::deciding_fix(version, &mut row.fixed_in);
        }
    }
    let compared = notes.trust.as_ref().is_some_and(Trust::compared_in_full);
    let needs_release = ecosystem::needs_release(package.ecosystem)?;
    let mut data = respond(
        package,
        &confirmed,
        notes,
        &results,
        &upgrade,
        needs_release,
    );
    if let Some(github) = &github {
        github.apply(&mut data, &package.brief);
        crate::response::result::finish_calls(&mut data);
    }
    if let Some(error) = &osv_failed {
        osv_unanswered(&mut data, &confirmed, &results, github.is_some(), error);
        // The rewrite replaced the answer: it names the review cautions again.
        answer::name_review_cautions(&mut data);
    }
    // After every other caution, so the answer names them all.
    malware_caution(&mut data, github.as_ref(), &results, &confirmed);
    paging::push_warnings(&mut data, disputes);
    if let Some(why) = probe_failed {
        paging::push_warnings(
            &mut data,
            [format!(
                "Asking OSV by name whether this release lists the package failed ({why}), so the empty answer at this version stays not covered."
            )],
        );
    }
    if resumed {
        // The registry facts were read with the first page; this page continues
        // OSV's walk and, on its last page, compares the repository's advisories.
        // The comparison is claimed only when it ran: OSV's list whole and
        // the repository's list read in full.
        let this_page = match (finished, compared) {
            (true, true) => {
                "this page ends the advisory list and compares the source repository's advisories with it"
            }
            (true, false) => "this page ends the advisory list",
            (false, _) => "this page continues the advisory list only",
        };
        paging::push_warnings(
            &mut data,
            [format!(
                "Trust, repository and update facts are on the first page of this walk; {this_page}."
            )],
        );
    }
    Ok(in_view(package, data, &upgrade))
}

/// The databases that dispute a listed advisory, read for `details` only:
/// the summary states no record's standing in another database, and a
/// dispute changes neither the verdict, the count nor the upgrade.
async fn disputes(
    view: SecurityCheckView,
    confirmed: &Confirmed,
    results: &[SourceResult],
    net: &Net<'_>,
) -> dispute::Read {
    match view {
        SecurityCheckView::Details => dispute::read(confirmed, results, net).await,
        SecurityCheckView::Summary => dispute::Read::default(),
    }
}

/// The finished row as the package's view states it.
fn in_view(package: &Package<'_>, data: Value, upgrade: &Upgrade) -> Value {
    summary::apply(
        package.view,
        data,
        summary::Target {
            query: package_row(package, package.name, package.version),
            asked_name: Some(package.name),
            asked_version: package.version,
            unfixed_in_major: upgrade.unfixed_in_major,
            gives_verdict: true,
        },
    )
}

/// A row whose OSV read failed: OSV is a failed source, the row partial, and
/// the answer says the target was not checked against OSV. Nothing is counted
/// (`affected` is OSV's word) and nothing is called none known; GitHub's
/// records, when read, stand as its own.
fn osv_unanswered(
    data: &mut Value,
    confirmed: &Confirmed,
    results: &[SourceResult],
    github_read: bool,
    error: &ArtifactError,
) {
    let target = answer::typed(&match confirmed.version.as_deref() {
        Some(version) => format!("{}@{version}", confirmed.name),
        None => confirmed.name.clone(),
    });
    let why = format!("{}. Retry later.", super::osv::unread_reason(error));
    let ids = results
        .iter()
        .flat_map(|result| &result.advisories)
        .map(|row| answer::stated_id(&row.id))
        .collect::<Vec<_>>();
    let github = match (github_read, ids.len()) {
        (false, _) => "GitHub's advisory database was not read for it.".to_owned(),
        (true, 0) => {
            "GitHub's advisory database matches none to it, which proves only that database."
                .to_owned()
        }
        (true, 1) => format!(
            "GitHub's advisory database matches 1 advisory to it, GitHub's word: {}.",
            ids.join(", ")
        ),
        (true, count) => format!(
            "GitHub's advisory database matches {count} advisories to it, GitHub's word: {}.",
            ids.join(", ")
        ),
    };
    if let Some(row) = data.as_object_mut() {
        for key in ["affected", "status", "upgradeTo", "upgradeNote"] {
            row.remove(key);
        }
        if ids.is_empty() {
            row.remove("advisories");
        }
    }
    let osv = if super::osv::answered_unreadably(error) {
        "OSV's answer could not be read"
    } else {
        "OSV did not answer"
    };
    data["answer"] = json!(format!(
        "{osv}, so {target} was not checked against it. {github}"
    ));
    let entry = json!({"source": "osv", "status": "failed", "errorCode": error.code, "why": why});
    match data["sources"].as_array_mut() {
        Some(sources) => sources.insert(0, entry),
        None => data["sources"] = json!([entry]),
    }
    // OSV's failure is one a retry may clear, as its error said before.
    data["retryable"] = json!(true);
    // The OSV entry states why; the warnings do not repeat it.
    super::source::source_failed(data, &[]);
}

/// The `malware` caution of a package row, from GitHub's class of a record
/// it matches to the version and OSV's malicious-packages reports alike.
fn malware_caution(
    data: &mut Value,
    github: Option<&global::Global>,
    results: &[SourceResult],
    confirmed: &Confirmed,
) {
    let mut reports = github
        .map(|github| {
            github
                .malware_ids()
                .iter()
                .map(|id| super::malware::Report::github(id))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    reports.extend(super::malware::reports(results));
    super::malware::fire(
        data,
        &reports,
        confirmed.version.is_some(),
        super::malware::npm_placeholder(
            &confirmed.ecosystem.to_string(),
            confirmed.version.as_deref(),
        ),
    );
}

/// Whether the release asked is one a source names. Asked only when a version
/// asked of a release came back with nothing, once, by name: any record of the
/// package in that release makes the empty answer at the version a real one.
async fn release_named(
    confirmed: &Confirmed,
    results: &[SourceResult],
    net: &Net<'_>,
) -> Result<bool, ArtifactError> {
    let answered = results
        .iter()
        .any(|result| result.resume.is_some() || !result.advisories.is_empty());
    if confirmed.release.is_none() || confirmed.version.is_none() || answered {
        return Ok(false);
    }
    names_release(confirmed, net).await
}

/// What the sources say to upgrade to.
#[derive(Debug, Default)]
struct Upgrade {
    /// The lowest version the sources say clears every listed advisory.
    to: Option<String>,
    /// Why none is named, or what OSV lists at that version besides.
    note: Option<String>,
    /// Listed advisories only a later major than the version's clears.
    unfixed_in_major: usize,
}

impl Upgrade {
    /// No upgrade is named, and `note` says why.
    fn none(note: String) -> Self {
        Self {
            note: Some(note),
            ..Self::default()
        }
    }

    /// `named` clears every advisory of `counted` (the row's own list);
    /// `listed` adds the ones OSV lists on the way there.
    fn to(
        version: &str,
        named: String,
        counted: &[SecurityAdvisory],
        listed: &[SecurityAdvisory],
        besides: Option<String>,
    ) -> Self {
        let gap = upgrade::major_gap(version, &named, listed);
        let unfixed = upgrade::unfixed_in_major(version, &named, counted);
        // Advisories, not records: a GHSA record and its twin are one.
        let records = counted.iter().collect::<Vec<_>>();
        let unfixed_in_major = super::record::advisory_groups(&records)
            .iter()
            .filter(|group| group.iter().any(|row| unfixed.contains(&row.id.as_str())))
            .count();
        Self {
            note: match (gap, besides) {
                (Some(gap), Some(besides)) => Some(format!("{gap} {besides}")),
                (gap, besides) => gap.or(besides),
            },
            to: Some(named),
            unfixed_in_major,
        }
    }
}

/// The candidate from the listed fixes, then OSV's own answer at it. Another
/// advisory there moves the candidate past that one's fix, until OSV knows none
/// at it or no version can be named. A listed advisory still there names none.
async fn upgrade_for(confirmed: &Confirmed, results: &[SourceResult], net: &Net<'_>) -> Upgrade {
    // A withdrawn record names no upgrade: it is not counted.
    let records = results
        .iter()
        .flat_map(|result| &result.advisories)
        .collect::<Vec<_>>();
    let mut listed = super::record::advisory_groups(&records)
        .iter()
        .flat_map(|group| super::record::Standing::of(group).live)
        .cloned()
        .collect::<Vec<_>>();
    let finished = results.iter().all(|result| result.resume.is_none());
    let (Some(version), false, true) = (confirmed.version.as_deref(), listed.is_empty(), finished)
    else {
        return Upgrade::default();
    };
    let counted = listed.clone();
    let mut named = match upgrade::candidate(version, &listed) {
        Ok(candidate) => candidate,
        Err(why) => return Upgrade::none(format!("No upgrade named: {why}.")),
    };
    loop {
        let probe = Target::Package(Confirmed {
            ecosystem: confirmed.ecosystem,
            name: confirmed.name.clone(),
            version: Some(named.clone()),
            resume: None,
            release: confirmed.release.clone(),
            import: confirmed.import.clone(),
        });
        let answers = match ask_all(&probe, net).await {
            Ok(answers) if answers.iter().all(|result| result.resume.is_none()) => answers,
            _ => {
                return Upgrade::none(format!(
                    "No upgrade named: {named} is the highest listed fix, but OSV could not confirm it."
                ));
            }
        };
        let at_named = answers
            .into_iter()
            .flat_map(|result| result.advisories)
            .collect::<Vec<_>>();
        if at_named.is_empty() {
            return Upgrade::to(version, named, &counted, &listed, None);
        }
        if let Some(still) = at_named
            .iter()
            .find(|row| listed.iter().any(|known| known.id == row.id))
        {
            return Upgrade::none(format!(
                "No upgrade named: OSV still lists {} at {named}, the highest listed fix.",
                still.id
            ));
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
                let others = format!("OSV lists other advisories at {named}: {}.", ids.join(", "));
                return Upgrade::to(version, named, &counted, &listed, Some(others));
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
    let Some(kind) = ecosystem::registry(package.ecosystem)? else {
        // No registry reads this ecosystem: OSV hears the name as given.
        return Ok((
            Confirmed {
                ecosystem: package.ecosystem,
                name: name.to_owned(),
                version: package.version.map(str::to_owned),
                resume: None,
                release: package.release.map(str::to_owned),
                import: None,
            },
            Notes {
                unconfirmed: true,
                ..Notes::default()
            },
        ));
    };
    let resolve = resolve_query(kind, name, package.version)?;
    let page = match execute_artifact(&resolve, &context(net)).await {
        Ok(page) => page,
        // The package is public and this version is not: the source may hear
        // the name, never a version the registry does not list.
        Err(error) if error.code == ErrorCode::VersionNotFound => {
            return Ok((
                Confirmed {
                    ecosystem: package.ecosystem,
                    name: name.to_owned(),
                    version: None,
                    resume: None,
                    release: package.release.map(str::to_owned),
                    import: None,
                },
                Notes {
                    absent: Some(error),
                    ..Notes::default()
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
    // A pinned version asks the registry once more for the latest, in `run`,
    // alongside the other reads; every other row already holds it.
    let (latest, latest_lookup) = latest_of(artifact, package, kind);
    Ok((
        Confirmed {
            ecosystem: package.ecosystem,
            name: source_name(artifact),
            version: artifact.version.clone(),
            resume: None,
            release: package.release.map(str::to_owned),
            import: go_import(artifact),
        },
        Notes {
            absent: None,
            unconfirmed: false,
            pinned: exact_pins(artifact),
            registered: Some(Registered::of(artifact)),
            current_published_at: artifact.published_at.clone(),
            latest,
            latest_lookup,
            latest_unread: None,
            trust: None,
            repository_unread: None,
            release_named: false,
            prerelease_warning: artifact.only_prerelease.then(|| {
                format!(
                    "{name}: the registry lists no stable release, so this checks its newest prerelease."
                )
            }),
        },
    ))
}

/// What the registry states of the confirmed version, read again on the last
/// page of a resumed walk: the source repository is the registry's to name,
/// and the cursor carries only OSV's walk. An ecosystem no registry reads has
/// none to name.
async fn registered_again(
    package: &Package<'_>,
    net: &Net<'_>,
) -> Result<Option<Registered>, Unread> {
    let unread = |code: ErrorCode, message: &str| Unread {
        code,
        why: format!(
            "{}: the registry was not read again for its source repository, so that repository's advisories were not compared with OSV's records: {}",
            package.name,
            sentence(message)
        ),
    };
    let Some(kind) = ecosystem::registry(package.ecosystem)
        .map_err(|error| unread(error.code, &error.message))?
    else {
        return Ok(None);
    };
    let query = resolve_query(kind, package.name, package.version)
        .map_err(|error| unread(error.code, &error.message))?;
    let page = execute_artifact(&query, &context(net))
        .await
        .map_err(|error| unread(error.code, &error.message))?;
    page.artifacts
        .first()
        .map(|artifact| Some(Registered::of(artifact)))
        .ok_or_else(|| {
            unread(
                ErrorCode::NotFound,
                "the registry no longer lists this version",
            )
        })
}

/// The registry lookup of `name` at `version`, as the artifact tool validates it.
fn resolve_query(
    kind: octocode_contract_types::ArtifactType,
    name: &str,
    version: Option<&str>,
) -> Result<ArtifactSearchQuery, ArtifactError> {
    let mut resolve = json!({
        "operation": "resolve",
        "ecosystem": artifact_type::as_str(kind),
        "packageName": name,
    });
    if let Some(version) = version {
        resolve["version"] = json!(version);
    }
    let resolve = crate::contracts::validate_query(ToolId::ArtifactSearch.as_str(), resolve)
        .map_err(|error| {
            ArtifactError::new(
                ErrorCode::InvalidInput,
                format!("{name} is not a {kind} package coordinate: {error}"),
            )
        })?;
    serde_json::from_value(resolve)
        .map_err(|_| ArtifactError::new(ErrorCode::InvalidInput, "Invalid package query."))
}

/// The anonymous registry context: no authorization, no configured registry.
fn context<'a>(net: &'a Net<'a>) -> ArtifactProviderContext<'a> {
    ArtifactProviderContext {
        http: net.http,
        budget: net.budget,
        npm_registry: None,
        allow_private_registry: false,
        answers: net.answers,
        reused: net.reused,
        tags: None,
    }
}

/// The registry's latest stable release when the confirmed artifact already
/// states it (no version asked), and the registry to ask when it does not. A
/// registry with only prereleases has no latest.
fn latest_of(
    artifact: &ArtifactItem,
    package: &Package<'_>,
    kind: octocode_contract_types::ArtifactType,
) -> (
    Option<Latest>,
    Option<octocode_contract_types::ArtifactType>,
) {
    if artifact.only_prerelease {
        return (None, None);
    }
    if package.version.is_some() {
        return (None, Some(kind));
    }
    let latest = artifact.version.clone().map(|version| Latest {
        version,
        published_at: artifact.published_at.clone(),
        repository: artifact.repository.clone(),
    });
    (latest, None)
}

/// One more unversioned lookup of the registry's latest stable release; a
/// failed lookup says why it is unknown.
async fn read_latest(
    package: &Package<'_>,
    kind: octocode_contract_types::ArtifactType,
    net: &Net<'_>,
) -> Result<Option<Latest>, ArtifactError> {
    let Ok(unversioned) = resolve_query(kind, package.name, None) else {
        return Ok(None);
    };
    let page = execute_artifact(&unversioned, &context(net)).await?;
    Ok(page.artifacts.first().and_then(|item| {
        item.version.clone().map(|version| Latest {
            version,
            published_at: item.published_at.clone(),
            repository: item.repository.clone(),
        })
    }))
}

/// The trust read of the last page of a resumed walk: the registry is read
/// again for the source repository, and one GitHub lacks is followed to the
/// repository the latest release names. A registry read that fails is the
/// row's failed source, never a comparison claimed.
async fn last_page_trust(
    package: &Package<'_>,
    confirmed: &Confirmed,
    notes: &mut Notes,
    net: &Net<'_>,
) {
    let registered = match registered_again(package, net).await {
        Ok(Some(registered)) => registered,
        Ok(None) => return,
        Err(unread) => {
            notes.repository_unread = Some(unread);
            return;
        }
    };
    let mut trust = trust::read(registered, confirmed, net).await.on_last_page();
    if trust.repository_missing()
        && let Ok(Some(kind)) = ecosystem::registry(package.ecosystem)
    {
        match read_latest(package, kind, net).await {
            Ok(Some(latest)) => trust.follow(&latest, confirmed, net).await,
            Ok(None) => {}
            Err(error) => {
                notes.repository_unread = Some(latest_unread(
                    package,
                    &error,
                    "the advisories of the repository it names were not compared with OSV's records",
                ));
            }
        }
    }
    notes.trust = Some(trust);
}

/// A failed read of the latest release, with what stays unknown for it
/// (`unknown`): the registry is a failed source and the row is partial.
fn latest_unread(package: &Package<'_>, error: &ArtifactError, unknown: &str) -> Unread {
    Unread {
        code: error.code,
        why: format!(
            "{}: the latest release was not read, so {unknown}: {}",
            package.name,
            sentence(&error.message)
        ),
    }
}

/// The name a source keys this package under: the registry's own spelling, and
/// for Go the module, since OSV keys Go by module, not by package path.
fn source_name(artifact: &ArtifactItem) -> String {
    artifact
        .module_path
        .clone()
        .unwrap_or_else(|| artifact.package_name.clone())
}

/// The package path of a Go package inside its module: the import path OSV's
/// `ecosystem_specific.imports` lists. A module row has none.
fn go_import(artifact: &ArtifactItem) -> Option<String> {
    artifact
        .module_path
        .as_ref()
        .map(|_| artifact.package_name.clone())
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
    row.insert("ecosystem".into(), json!(package.ecosystem.to_string()));
    if let Some(release) = package.release {
        row.insert("release".into(), json!(release));
    }
    row.insert("packageName".into(), json!(name));
    if let Some(version) = version {
        row.insert("version".into(), json!(version));
    }
    row
}

/// Ready calls that check each exactly pinned dependency at its pinned
/// version: a package that bundles its parts (rails over actionpack) carries
/// their advisories under their own names, so its own row cannot hold them.
/// Each of those rows reports its own pins in turn.
fn pinned_calls(data: &mut Value, package: &Package<'_>, pins: &[Pin]) {
    let per_call = crate::response::result::rows_per_call(ToolId::SecurityCheck);
    for (index, chunk) in pins.chunks(per_call).enumerate() {
        let queries = chunk
            .iter()
            .map(|pin| Value::Object(package_row(package, &pin.name, Some(&pin.version))))
            .collect::<Vec<_>>();
        data["calls"][format!("pinnedDependencies{index}")] =
            Continuation::related_input(ToolId::SecurityCheck, json!({ "queries": queries }))
                .build();
    }
}

/// A row whose empty answer proves nothing: no `status:"empty"`, and each source
/// says it did not cover this target, never `noneKnown`.
fn not_evaluated(data: &mut Value, why: &str) {
    if let Some(row) = data.as_object_mut() {
        row.remove("status");
    }
    for source in data["sources"].as_array_mut().into_iter().flatten() {
        source["status"] = json!("notCovered");
        source["why"] = json!(why);
    }
}

/// The answer first, the target's facts, the sources' row, and what continues it.
fn respond(
    package: &Package<'_>,
    confirmed: &Confirmed,
    notes: Notes,
    results: &[SourceResult],
    upgrade: &Upgrade,
    needs_release: bool,
) -> Value {
    let name = confirmed.name.as_str();
    let version = confirmed.version.as_deref();
    let target = version.map_or_else(|| name.to_owned(), |version| format!("{name}@{version}"));
    let mut facts = answer::Facts::of(results);
    facts.upgrade_to = upgrade.to.as_deref();
    facts.upgrade_note = upgrade.note.as_deref();
    facts.version_evaluated = version.is_some();
    facts.all_versions = notes.unconfirmed && version.is_none();
    facts.release_named = notes.release_named;
    facts.release_unmatched =
        confirmed.release.is_some() && facts.count == 0 && !notes.release_named;
    facts.release_missing =
        needs_release && confirmed.release.is_none() && version.is_some() && facts.count == 0;
    facts.pinned_dependencies = notes.pinned.len();
    facts.registry_unconfirmed = notes.unconfirmed;
    let mut data = answer::lead(&target, &facts);
    data["packageName"] = json!(name);
    match version {
        Some(version) => data["version"] = json!(version),
        None if notes.unconfirmed => {}
        None => data["versionEvaluated"] = json!(false),
    }
    if notes.unconfirmed {
        data["registryConfirmed"] = json!(false);
    }
    let mut data = into_row(data, results, View::Brief);
    if !facts.stopped {
        // Not none known: OSV answers a release it does not name (no record of the
        // package in it, by name), and a version
        // asked with no release, the same way as a release with no advisory.
        if facts.release_unmatched {
            not_evaluated(
                &mut data,
                "OSV answers a release it does not name like a release with no advisory.",
            );
        } else if facts.release_missing {
            not_evaluated(
                &mut data,
                "This distribution matches versions per release and none was given.",
            );
        }
    }
    answer::read_whole(&mut data, results, &package.brief);
    pinned_calls(&mut data, package, &notes.pinned);
    paging::push_warnings(&mut data, notes.prerelease_warning);
    if let Some(unread) = &notes.latest_unread {
        registry_failed(&mut data, unread);
    }
    if let Some(unread) = &notes.repository_unread {
        registry_failed(&mut data, unread);
    }
    if let (Some(latest), Some(version)) = (&notes.latest, version) {
        update::apply(
            &mut data,
            version,
            notes.current_published_at.as_deref(),
            latest,
        );
    }
    if let Some(trust) = &notes.trust {
        trust.apply(&mut data, &package.brief);
    }
    if let Some(absent) = notes.absent {
        // A version the registry does not list is a failed lookup, so the row
        // exits non-zero; it still lists what the sources know of the name.
        data["status"] = json!("error");
        data["errorCode"] = json!(absent.code);
        data["error"] = json!(sentence(&absent.message));
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
        summary::carry(package.view, &mut next);
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
