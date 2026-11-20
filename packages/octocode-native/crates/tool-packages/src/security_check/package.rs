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
use crate::artifact::query::artifact_type;
use crate::artifact::{
    ArtifactError, ArtifactItem, ArtifactProviderContext, ArtifactProviderPage, Pin, exact_pins,
    execute_artifact, lookalike,
};
use octocode_contract_types::{ArtifactSearchQuery, ErrorCode, SecurityCheckView};
use octocode_runtime_base::id::ToolId;
use octocode_runtime_base::response::paging::{self, PartialReason};
use octocode_runtime_base::response::result::Continuation;
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
    /// The version `absent` names is one OSV's records name by version, so OSV
    /// was asked at it and the row answers ([`ask_unlisted`]).
    unlisted: bool,
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
    // npm's security placeholder names npm's holding repository, not the
    // package's source: no repository trust is read for it, and the row
    // says why.
    let placeholder = super::malware::npm_placeholder(
        &confirmed.ecosystem.to_string(),
        confirmed.version.as_deref(),
    )
    .filter(|_| registered.is_some())
    .map(str::to_owned);
    let target = Target::Package(confirmed.clone());
    let lookup = notes.latest_lookup.take();
    // Names one edit away are counted for a name the registry confirmed, on
    // the first page, as its trust is read.
    let near_kind = registered
        .as_ref()
        .and_then(|_| ecosystem::registry(package.ecosystem).ok().flatten());
    let (results, trust, latest, near) = tokio::join!(
        ask_all(&target, net),
        async {
            match registered {
                Some(registered) if placeholder.is_none() => {
                    Some(trust::read(registered, &confirmed, net).await)
                }
                _ => None,
            }
        },
        async {
            match lookup {
                Some(kind) => Some(read_latest(package, kind, net).await),
                None => None,
            }
        },
        async {
            match near_kind {
                Some(kind) => lookalike::near_names(kind, &confirmed.name, &context(net)).await,
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
    let (confirmed, results) = match osv_failed {
        None => ask_unlisted(package, confirmed, &mut notes, results, net).await,
        Some(_) => (confirmed, results),
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
        Err(error) => Some(octocode_runtime_base::security::quoted(&error.message).into_owned()),
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
        let order = upgrade::Order::of(ecosystem::version_order(confirmed.ecosystem));
        for row in results.iter_mut().flat_map(|result| &mut result.advisories) {
            upgrade::deciding_fix(version, &mut row.fixed_in, order);
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
        octocode_runtime_base::response::result::finish_calls(&mut data);
    }
    if let Some(error) = &osv_failed {
        let answered = github
            .as_ref()
            .map_or(global::Answered::Not, global::Global::answered);
        osv_unanswered(&mut data, &confirmed, &results, answered, error);
        // The rewrite replaced the answer: it names the review cautions again.
        answer::name_review_cautions(&mut data);
    }
    lookalike_caution(&mut data, &confirmed, near);
    if let Some(version) = placeholder {
        paging::push_warnings(
            &mut data,
            [format!(
                "{} {version} is npm's placeholder for a package it removed for security reasons: the repository it names is npm's holding repository, not this package's source, so no repository trust is stated.",
                confirmed.name
            )],
        );
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
            asked_name: Some(package.name),
            asked_version: package.version,
            unfixed_in_major: upgrade.unfixed_in_major,
            gives_verdict: true,
            upgrade_prerelease: upgrade.prerelease,
        },
    )
}

/// A row whose OSV read failed: OSV is a failed source, the row partial, and
/// the answer says the target was not checked against OSV. Nothing is counted
/// (`affected` is OSV's word) and nothing is called none known; GitHub's
/// records stand as its own, as far as its read reached (`github`): a read
/// that failed matches nothing, and one that stopped names a floor.
fn osv_unanswered(
    data: &mut Value,
    confirmed: &Confirmed,
    results: &[SourceResult],
    github: global::Answered,
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
    let github = match (github, ids.len()) {
        (global::Answered::Not, _) => "GitHub's advisory database was not read for it.".to_owned(),
        (global::Answered::InPart, 0) => {
            "GitHub's advisory database was read in part and matches none to it in what was read, which proves nothing."
                .to_owned()
        }
        (global::Answered::InPart, count) => format!(
            "GitHub's advisory database was read in part and matches at least {count} {} to it, GitHub's word: {}.",
            if count == 1 { "advisory" } else { "advisories" },
            ids.join(", ")
        ),
        (global::Answered::Whole, 0) => {
            "GitHub's advisory database matches none to it, which proves only that database."
                .to_owned()
        }
        (global::Answered::Whole, 1) => format!(
            "GitHub's advisory database matches 1 advisory to it, GitHub's word: {}.",
            ids.join(", ")
        ),
        (global::Answered::Whole, count) => format!(
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

/// The `lookalikeName` caution of a confirmed package whose registry has a
/// name one edit away with at least the policy's `lookalikeDownloadsRatio`
/// times its downloads; a count that could not be read says no lookalike
/// was checked.
fn lookalike_caution(
    data: &mut Value,
    confirmed: &Confirmed,
    near: Option<Result<lookalike::Near, ArtifactError>>,
) {
    let near = match near {
        None => return,
        Some(Ok(near)) => near,
        Some(Err(error)) => {
            paging::push_warnings(
                data,
                [format!(
                    "{}: the names one edit away could not be counted ({}), so no lookalike was checked.",
                    confirmed.name,
                    octocode_runtime_base::security::quoted(&error.message)
                )],
            );
            return;
        }
    };
    let Ok(contract) = octocode_runtime_base::contracts::security::security_contract() else {
        return;
    };
    let ratio = contract.policy.lookalike_downloads_ratio;
    let Some((other, count)) = near.lookalike(ratio) else {
        return;
    };
    let why = format!(
        "The registry counts {count} {} for {other}, one edit away, against {} for {} (at least {ratio} times, policy lookalikeDownloadsRatio).",
        near.measure,
        near.own.unwrap_or(0),
        confirmed.name
    );
    let Some(caution) = super::repository::caution(contract, "lookalikeName", why) else {
        return;
    };
    match data["cautions"].as_array_mut() {
        Some(cautions) => cautions.push(caution),
        None => data["cautions"] = json!([caution]),
    }
    answer::name_review_cautions(data);
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
    /// `to` is a prerelease of a stable version: no stable release above
    /// the deciding fix is published, which `note` says.
    prerelease: bool,
}

impl Upgrade {
    /// No upgrade is named, and `note` says why.
    fn none(note: String) -> Self {
        Self {
            note: Some(note),
            ..Self::default()
        }
    }

    /// `named` clears every advisory of `counted` (the row's own list). The
    /// major-line note names only those: an advisory OSV lists at a skipped
    /// candidate does not affect `version`, and `besides` says why it was skipped.
    fn to(
        version: &str,
        named: String,
        counted: &[SecurityAdvisory],
        besides: Option<String>,
        order: upgrade::Order,
    ) -> Self {
        let gap = upgrade::major_gap(version, &named, counted, order);
        let unfixed = upgrade::unfixed_in_major(version, &named, counted, order);
        // Advisories, not records: a GHSA record and its twin are one.
        let records = counted.iter().collect::<Vec<_>>();
        let unfixed_in_major = super::record::advisory_groups(&records)
            .iter()
            .filter(|group| group.iter().any(|row| unfixed.contains(&row.id.as_str())))
            .count();
        // A stable version moves to a prerelease only when no stable release
        // above the deciding fix is published: said first.
        let prerelease = !order.prerelease(version) && order.prerelease(&named);
        let only = prerelease.then(|| {
            format!(
                "Only a prerelease fix is listed: {named}; no stable release above it was found on the registry."
            )
        });
        let note = [only, gap, besides]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        Self {
            note: (!note.is_empty()).then(|| note.join(" ")),
            to: Some(named),
            unfixed_in_major,
            prerelease,
        }
    }
}

/// The candidate from the listed fixes, then OSV's own answer at it. Another
/// advisory there moves the candidate past that one's fix, until OSV knows none
/// at it or no version can be named. A listed advisory still there names none.
/// Fixed versions compare in the registry's version order; a prerelease
/// candidate of a stable version steps to the lowest stable release the
/// registry publishes above it.
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
    let order = upgrade::Order::of(ecosystem::version_order(confirmed.ecosystem));
    // A prerelease asked may move to another; a stable version moves to a
    // stable release whenever one clears the advisories.
    let prereleases = order.prerelease(version);
    let counted = listed.clone();
    // Each candidate passed over, with what OSV lists at it.
    let mut skipped: Vec<String> = Vec::new();
    // The registry's live releases, once a read listed them: later candidates
    // are checked against this list, never read again.
    let mut releases = Releases::default();
    let mut named = match upgrade::candidate(version, &listed, order) {
        Ok(candidate) => candidate,
        Err(why) => return Upgrade::none(format!("No upgrade named: {why}.")),
    };
    loop {
        if !prereleases && order.prerelease(&named) {
            let stable = releases
                .live(confirmed, &named, net)
                .await
                .and_then(|live| upgrade::next_published(&named, live, order, false));
            if let Some(stable) = stable {
                skipped.push(format!(
                    "{named} (a prerelease; {stable} is the lowest stable release above it)"
                ));
                named = stable;
            }
        }
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
        let ids = at_named
            .iter()
            .map(|row| row.id.as_str())
            .fold(Vec::new(), |mut ids, id| {
                if !ids.contains(&id) {
                    ids.push(id);
                }
                ids
            })
            .join(", ");
        if at_named.is_empty() {
            // OSV is clear at it; a release the registry does not publish, or
            // yanked, is no upgrade: step to the next one published.
            let Listing::Gone(published) = releases.listing(confirmed, &named, order, net).await
            else {
                return Upgrade::to(version, named, &counted, skipped_note(&skipped), order);
            };
            // A stable release first; a prerelease only when none is published.
            let Some(next) = upgrade::next_published(&named, &published, order, prereleases)
                .or_else(|| upgrade::next_published(&named, &published, order, true))
            else {
                return Upgrade::none(format!(
                    "No upgrade named: {named} is the highest listed fix, but the registry does not publish it and lists no later release."
                ));
            };
            skipped.push(format!(
                "{named} (not published on the registry, or yanked)"
            ));
            named = next;
            continue;
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
        match upgrade::candidate(&named, &at_named, order) {
            Ok(next) => {
                skipped.push(format!("{named} (OSV lists {ids} at it)"));
                listed.extend(at_named);
                named = next;
            }
            Err(_) => {
                let others = format!("OSV lists other advisories at {named}: {ids}.");
                let besides = match skipped_note(&skipped) {
                    Some(note) => format!("{note} {others}"),
                    None => others,
                };
                return Upgrade::to(version, named, &counted, Some(besides), order);
            }
        }
    }
}

/// What the registry says of one release.
enum Listing {
    /// Published and not yanked, or the registry could not say.
    Present,
    /// Not published, or yanked: the live releases the registry does list.
    Gone(Vec<String>),
}

/// The registry's live releases of one walk, read at most once: the first
/// read that lists them answers every later candidate.
#[derive(Default)]
struct Releases {
    /// The list was asked for once; a later candidate never asks again.
    read: bool,
    /// `None`: not read yet, or the read listed none.
    live: Option<Vec<String>>,
}

impl Releases {
    /// The live releases, read now when no earlier step listed them: every
    /// release the registry publishes, less those it marks yanked. `None`
    /// when the registry lists none or cannot say.
    async fn live(
        &mut self,
        confirmed: &Confirmed,
        version: &str,
        net: &Net<'_>,
    ) -> Option<&[String]> {
        if !self.read {
            self.read = true;
            self.live = published_releases(confirmed, version, net).await;
        }
        self.live.as_deref()
    }

    /// Whether the registry publishes `version`, not yanked: from the live
    /// releases once listed, else through the same resolve path [`confirm`]
    /// reads. A registry that fails to answer, or an ecosystem with no
    /// registry reader, leaves the release as OSV cleared it.
    async fn listing(
        &mut self,
        confirmed: &Confirmed,
        version: &str,
        order: upgrade::Order,
        net: &Net<'_>,
    ) -> Listing {
        if let Some(live) = &self.live {
            return if live.iter().any(|release| order.same(release, version)) {
                Listing::Present
            } else {
                Listing::Gone(live.clone())
            };
        }
        let Ok(Some(kind)) = ecosystem::registry(confirmed.ecosystem) else {
            return Listing::Present;
        };
        match read_release(kind, confirmed, version, net).await {
            // Yanked: the list is read next.
            Ok(page)
                if page
                    .artifacts
                    .first()
                    .is_some_and(|row| row.yanked == Some(true)) => {}
            Ok(_) => return Listing::Present,
            // Not published: the answer lists what is.
            Err(error) if error.missing_version => {
                self.read = true;
                self.live = live_of(error);
            }
            Err(_) => return Listing::Present,
        }
        let live = self.live(confirmed, version, net).await;
        Listing::Gone(live.map(<[String]>::to_vec).unwrap_or_default())
    }
}

/// The registry's lookup of `name` at `version`, anonymous, as [`confirm`] reads it.
async fn read_release(
    kind: octocode_contract_types::ArtifactType,
    confirmed: &Confirmed,
    version: &str,
    net: &Net<'_>,
) -> Result<ArtifactProviderPage, ArtifactError> {
    let resolve = resolve_query(kind, &confirmed.name, Some(version))?;
    execute_artifact(&resolve, &context(net)).await
}

/// Every release the registry publishes, less those it marks yanked: read by
/// resolving a version no registry can hold beside `version`, which a
/// registry answers with its list. `None` when there is no registry reader,
/// the read fails, or it lists nothing.
async fn published_releases(
    confirmed: &Confirmed,
    version: &str,
    net: &Net<'_>,
) -> Option<Vec<String>> {
    let Ok(Some(kind)) = ecosystem::registry(confirmed.ecosystem) else {
        return None;
    };
    let Err(error) = read_release(kind, confirmed, &format!("{version}-unpublished"), net).await
    else {
        return None;
    };
    live_of(error)
}

/// The live releases a missing version's answer lists: its versions less
/// those the registry marks yanked. `None` for any other failure, or none.
fn live_of(error: ArtifactError) -> Option<Vec<String>> {
    let live = error
        .nearest
        .into_iter()
        .filter(|release| !error.yanked.contains(release))
        .collect::<Vec<_>>();
    (error.missing_version && !live.is_empty()).then_some(live)
}

/// Why lower candidates were passed over, in one sentence; `None` when none was.
fn skipped_note(skipped: &[String]) -> Option<String> {
    (!skipped.is_empty()).then(|| format!("Skipped {}.", skipped.join("; ")))
}

/// A package the registry does not list: the recovery is the name's own
/// spelling, so the hint says that. Names one edit away the registry has
/// ([`lookalike`]) are checked instead, most downloaded first; a scoped npm
/// name searches its scope's packages; elsewhere, where the registry has a
/// keyword search, a related `artifactSearch` call searches the name.
fn not_on_registry(
    kind: octocode_contract_types::ArtifactType,
    package: &Package<'_>,
    near: Option<lookalike::Near>,
) -> ArtifactError {
    let name = package.name;
    let mut error = ArtifactError::new(
        ErrorCode::NotFound,
        format!(
            "{name} is not on the public {kind} registry, so it was not sent to OSV: advisories were not checked."
        ),
    )
    .with_status(404);
    error.hints = vec!["Check the name and ecosystem as the manifest spells them.".to_owned()];
    if let Some(near) = near.filter(|near| !near.names.is_empty()) {
        let limit =
            octocode_runtime_base::contracts::batch_limit(ToolId::SecurityCheck).unwrap_or(1);
        let read = &near.names[..near.names.len().min(limit)];
        error.message = format!(
            "{} The registry has {} one edit away{}.",
            error.message,
            read.iter()
                .map(|(other, count)| format!("{other} ({count} {})", near.measure))
                .collect::<Vec<_>>()
                .join(", "),
            match near.names.len() - read.len() {
                0 => String::new(),
                more => format!(", and {more} more with fewer"),
            }
        );
        let queries = read
            .iter()
            .map(|(other, _)| {
                let mut row = package.brief.clone();
                row.insert("operation".into(), json!("package"));
                row.insert("ecosystem".into(), json!(package.ecosystem.to_string()));
                row.insert("packageName".into(), json!(other));
                Value::Object(row)
            })
            .collect::<Vec<_>>();
        error.calls = Some(Box::new(json!({
            "repair": Continuation::related_input(ToolId::SecurityCheck, json!({ "queries": queries })).build()
        })));
        return error;
    }
    // PyPI has no keyword search to broaden an unknown name into.
    if kind != octocode_contract_types::ArtifactType::Pypi {
        // A scoped npm name: npm's search lists its scope's packages.
        let keyword = match kind {
            octocode_contract_types::ArtifactType::Npm if name.starts_with('@') => {
                name.split('/').next().unwrap_or(name)
            }
            _ => name,
        };
        let mut search = package.brief.clone();
        search.insert("operation".into(), json!("search"));
        search.insert("ecosystem".into(), json!(kind));
        search.insert("keywords".into(), json!([keyword]));
        error.calls = Some(Box::new(json!({
            "broadenSearch": Continuation::related(ToolId::ArtifactSearch, Value::Object(search)).build()
        })));
    }
    error
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
        Err(mut error) if error.missing_version => {
            // The repair call reads as many nearest versions as one call carries.
            let limit =
                octocode_runtime_base::contracts::batch_limit(ToolId::SecurityCheck).unwrap_or(1);
            let (nearest, fewer) = crate::artifact::suggested(kind, &error, limit);
            error.nearest = nearest;
            error.hints.extend(fewer);
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
        let near = match lookalike::near_names(kind, name, &context(net)).await {
            Some(Ok(near)) => Some(near),
            _ => None,
        };
        return Err(not_on_registry(kind, package, near));
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
            unlisted: false,
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
    let resolve =
        octocode_runtime_base::contracts::validate_query(ToolId::ArtifactSearch.as_str(), resolve)
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
        share: Some(net.share),
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
    let per_call = octocode_runtime_base::response::result::rows_per_call(ToolId::SecurityCheck);
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

/// The advisories the sources listed, as the answer counts them.
fn facts_count(results: &[SourceResult]) -> usize {
    answer::Facts::of(results).count
}

/// A requested version that is a range (`^4.17.0`, `>=1, <2`, `4.x`) resolves
/// to one version, its newest match: the rest of the range was not checked,
/// and the answer says only that version's verdict.
fn range_warning(requested: Option<&str>, resolved: Option<&str>) -> Option<String> {
    let (requested, resolved) = (requested?, resolved?);
    let ranged = requested != resolved
        && (requested.contains(['^', '~', '<', '>', '*', '|', ',', ' '])
            || requested
                .split('.')
                .any(|part| part.eq_ignore_ascii_case("x")));
    ranged.then(|| {
        format!(
            "version {requested} is a range: only {resolved}, its newest match, was checked; lower versions in the range were not. Name one exact version to check it."
        )
    })
}

/// The ids of the advisories whose records list `version` by name in an
/// `affected` entry, each once, in source order.
fn listing_version(results: &[SourceResult], version: &str) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for advisory in results.iter().flat_map(|result| &result.advisories) {
        let listed = advisory.affected.iter().any(|entry| {
            entry["versions"]
                .as_array()
                .is_some_and(|versions| versions.iter().any(|listed| listed == version))
        });
        if listed && !ids.contains(&advisory.id) {
            ids.push(advisory.id.clone());
        }
    }
    ids
}

/// A version the registry does not list (unpublished, removed for malware)
/// that a record OSV returned for the package names by that exact version: the
/// version is public in OSV's own record, so OSV is asked at it, and its match
/// answers the row ([`Notes::unlisted`]). Any other unlisted version is never
/// sent ([`confirm`]). A walk that did not finish, or an answer at the version
/// that failed, stopped or came back empty, leaves the by-name read, and the
/// row stays a failed lookup.
async fn ask_unlisted(
    package: &Package<'_>,
    confirmed: Confirmed,
    notes: &mut Notes,
    results: Vec<SourceResult>,
    net: &Net<'_>,
) -> (Confirmed, Vec<SourceResult>) {
    let finished = |results: &[SourceResult]| results.iter().all(|result| result.resume.is_none());
    let requested = package
        .version
        .filter(|_| notes.absent.is_some() && confirmed.version.is_none());
    let Some(requested) = requested
        .filter(|requested| finished(&results) && !listing_version(&results, requested).is_empty())
    else {
        return (confirmed, results);
    };
    let asked = Confirmed {
        version: Some(requested.to_owned()),
        ..confirmed.clone()
    };
    match ask_all(&Target::Package(asked.clone()), net).await {
        Ok(at_version)
            if finished(&at_version)
                && at_version
                    .iter()
                    .any(|result| !result.advisories.is_empty()) =>
        {
            notes.unlisted = true;
            (asked, at_version)
        }
        _ => (confirmed, results),
    }
}

/// The ids of the advisories `results` count, each advisory once by its
/// first live record; a withdrawn one is not counted.
fn counted_ids(results: &[SourceResult]) -> Vec<String> {
    let records = results
        .iter()
        .flat_map(|result| &result.advisories)
        .collect::<Vec<_>>();
    super::record::advisory_groups(&records)
        .iter()
        .map(|group| super::record::Standing::of(group))
        .filter(|standing| !standing.is_withdrawn())
        .filter_map(|standing| standing.live.first().map(|row| row.id.clone()))
        .collect()
}

/// A row whose empty answer proves nothing: each source says it did not cover
/// this target, never `noneKnown`.
fn not_evaluated(data: &mut Value, why: &str) {
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
    // A version the registry does not hold is still named in the records
    // OSV returned for the package: one listing it does affect it.
    if version.is_none()
        && let Some(requested) = package.version
    {
        facts.unregistered = Some((requested, listing_version(results, requested)));
    }
    let mut data = answer::lead(&target, &facts);
    data["packageName"] = json!(name);
    match version {
        // OSV was asked at a version the registry does not list: no version
        // is the registry's, and this field says so.
        Some(_) if notes.unlisted => data["versionListed"] = json!(false),
        Some(version) => data["version"] = json!(version),
        None if notes.unconfirmed => {}
        None => data["versionEvaluated"] = json!(false),
    }
    if notes.unconfirmed {
        data["registryConfirmed"] = json!(false);
    }
    let mut data = into_row(data, results, View::of(package.view));
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
    answer::read_disputing(&mut data, results, &package.brief);
    pinned_calls(&mut data, package, &notes.pinned);
    paging::push_warnings(&mut data, notes.prerelease_warning);
    // Stated once per call however many rows it holds for.
    paging::push_warnings(&mut data, answer::unconfirmed_warning(&facts));
    paging::push_warnings(&mut data, range_warning(package.version, version));
    // A distribution matches versions per release: with none given, OSV's
    // answer counts what every release lists for the version.
    if confirmed.release.is_none()
        && ecosystem::takes_release(package.ecosystem).unwrap_or(false)
        && facts_count(results) > 0
    {
        paging::push_warnings(
            &mut data,
            ["No release was given, so the advisories every release of this distribution lists are counted together, including ones fixed in yours: pass release (forms: 12, 22.04:LTS, v3.18) for one release's verdict.".to_owned()],
        );
    }
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
        let nearest = absent
            .nearest
            .iter()
            .map(|near| Value::Object(package_row(package, name, Some(near))))
            .collect::<Vec<_>>();
        let nearest = (!nearest.is_empty()).then(|| {
            Continuation::related_input(ToolId::SecurityCheck, json!({ "queries": nearest }))
                .build()
        });
        match version {
            // OSV answered at a version its records name: an answer, with a
            // check of the nearest published versions to move to.
            Some(version) if notes.unlisted => {
                paging::push_warnings(
                    &mut data,
                    [answer::unlisted_warning(
                        name,
                        version,
                        &counted_ids(results),
                    )],
                );
                if let Some(nearest) = nearest {
                    data["calls"]["otherVersions"] = nearest;
                }
            }
            // A version the registry does not list is a failed lookup, so the
            // row exits non-zero; it still lists what the sources know of the
            // name, and its repair reads the nearest published versions.
            _ => {
                data["status"] = json!("error");
                data["errorCode"] = json!(absent.code);
                data["error"] = json!(sentence(&absent.message));
                octocode_runtime_base::response::result::add_hints(
                    &mut data,
                    absent.hints.iter().cloned(),
                );
                if let Some(nearest) = nearest {
                    data["calls"]["repair"] = nearest;
                }
            }
        }
    }
    if let Some(resume) = results.iter().find_map(|result| result.resume.as_ref()) {
        let mut next = package_row(package, name, version);
        summary::carry(package.view, &mut next);
        next.insert("resume".into(), json!(resume.token));
        octocode_runtime_base::response::collection::insert_row_walk(
            &mut data,
            ToolId::SecurityCheck,
            Continuation::continued(ToolId::SecurityCheck, Value::Object(next)),
        );
        paging::mark_partial(
            &mut data,
            PartialReason::ProviderIncompleteResults,
            &[format!(
                "{} next.nextPage reads on from there.",
                resume.reason
            )],
        );
    }
    octocode_runtime_base::response::result::finish_calls(&mut data);
    data
}

#[cfg(test)]
#[path = "package_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "package_upgrade_tests.rs"]
mod upgrade_tests;
