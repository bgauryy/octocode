//! The `repository` row: what GitHub says of a repository (stars, last push,
//! archived, missing or moved) and the cautions the contract's rules and
//! thresholds draw from it. Facts, never a verdict; a GitHub that did not
//! answer leaves the row partial (`sourceFailed`), never clean.
use super::Net;
use super::advisories::{self, AdvisoryFuture, AdvisoryRead, GhsaFuture, GhsaRead};
use super::answer::typed;
use super::commit::{CommitFuture, CommitLookup};
use super::record::Selection;
use crate::contracts::security::{CautionBasis, CautionLevel, SecurityContract, security_contract};
use crate::id::ToolId;
use crate::providers::artifact::ArtifactError;
use crate::response::paging::{self, PartialReason};
use crate::response::result::Continuation;
use octocode_contract_types::{ErrorCode, SecurityCheckView};
use serde_json::{Map, Value, json};
use std::{future::Future, pin::Pin};

const DAY_MILLIS: i64 = 86_400_000;

/// Whether GitHub says a repository is public.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Visibility {
    Public,
    /// GitHub's `private` flag, or a `private` or `internal` visibility.
    NotPublic,
    /// GitHub's answer states neither (some GitHub Enterprise servers), or
    /// nothing was read: not known to be public, so it is treated as not
    /// public. The default, so a missed field never reads as public.
    #[default]
    Unknown,
}

impl Visibility {
    /// What GitHub's `private` flag and `visibility` text say together.
    pub(crate) fn of(private: Option<bool>, visibility: &str) -> Self {
        match (private, visibility) {
            (Some(true), _) | (_, "private" | "internal") => Self::NotPublic,
            (Some(false), _) | (None, "public") => Self::Public,
            (None, _) => Self::Unknown,
        }
    }

    /// A repository's published text is for a third party to hear only when
    /// GitHub says it is public.
    pub(crate) fn third_party_may_hear(self) -> bool {
        self == Self::Public
    }
}

/// A repository as GitHub described it.
#[derive(Clone, Debug, Default)]
pub(crate) struct RepoFacts {
    pub owner: String,
    pub name: String,
    pub url: String,
    pub stars: usize,
    pub forks: usize,
    pub pushed_at: Option<String>,
    pub archived: bool,
    pub fork: bool,
    /// What it publishes is not for a third party to hear unless it is public.
    pub visibility: Visibility,
    pub default_branch: String,
    pub license: Option<String>,
}

/// A GitHub read that did not answer: its refusal's code and what to do.
#[derive(Clone, Debug)]
pub(crate) struct RepoFailure {
    pub code: ErrorCode,
    pub why: String,
}

#[derive(Clone, Debug)]
pub(crate) enum RepoRead {
    Found(RepoFacts),
    /// GitHub redirected to the facts' repository: renamed or transferred.
    Moved(RepoFacts),
    /// GitHub has no such repository (deleted, private, or never there).
    Missing,
    Failed(RepoFailure),
}

pub(crate) type RepoFuture<'a> = Pin<Box<dyn Future<Output = RepoRead> + Send + 'a>>;

/// The GitHub reads a row needs, through the runtime's own GitHub transport,
/// credential and budget.
pub(crate) trait RepositoryRead: Send + Sync {
    fn read<'a>(&'a self, owner: &'a str, repo: &'a str) -> RepoFuture<'a>;

    /// The commit `sha` and where it lives; `default_branch` is the branch its
    /// history is checked against (empty: not checked).
    fn commit<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        sha: &'a str,
        default_branch: &'a str,
    ) -> CommitFuture<'a>;

    /// One page of the repository's published advisories: the first, or the one
    /// after `after`, a cursor a page named.
    fn advisories<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        after: Option<&'a str>,
    ) -> AdvisoryFuture<'a>;

    /// One page of GitHub's global advisory database for `affects`
    /// (`name@version`) in `ecosystem` (GitHub's name): the first, or the one
    /// after `after`.
    fn global_advisories<'a>(
        &'a self,
        ecosystem: &'a str,
        affects: &'a str,
        after: Option<&'a str>,
    ) -> AdvisoryFuture<'a>;

    /// One advisory by its GHSA id: the repository's own with `repo`, else
    /// the one GitHub's global database keeps.
    fn ghsa<'a>(&'a self, repo: Option<(&'a str, &'a str)>, id: &'a str) -> GhsaFuture<'a>;

    /// A request this read built carried no credential: GitHub answered
    /// with public data only, under the anonymous rate limit.
    fn anonymous(&self) -> bool;
}

/// GitHub's limits on an account name (an Enterprise Managed User login,
/// `handle_shortcode`, holds an underscore) and a repository name.
const MAX_OWNER_CHARS: usize = 39;
const MAX_NAME_CHARS: usize = 100;

fn is_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.len() <= MAX_OWNER_CHARS
        && owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
}

fn is_name(name: &str) -> bool {
    !matches!(name, "" | "." | "..")
        && name.len() <= MAX_NAME_CHARS
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The read of a row whose GitHub transport could not be built: it fails with
/// the reason, so the row says why GitHub was not asked.
pub(crate) struct FailedRepository(pub String);

impl FailedRepository {
    fn failure(&self) -> RepoFailure {
        RepoFailure {
            code: ErrorCode::ProviderError,
            why: self.0.clone(),
        }
    }
}

impl RepositoryRead for FailedRepository {
    fn read<'a>(&'a self, _: &'a str, _: &'a str) -> RepoFuture<'a> {
        Box::pin(async move { RepoRead::Failed(self.failure()) })
    }

    fn commit<'a>(&'a self, _: &'a str, _: &'a str, _: &'a str, _: &'a str) -> CommitFuture<'a> {
        Box::pin(async move { CommitLookup::Failed(self.failure()) })
    }

    fn advisories<'a>(&'a self, _: &'a str, _: &'a str, _: Option<&'a str>) -> AdvisoryFuture<'a> {
        Box::pin(async move { AdvisoryRead::Failed(self.failure()) })
    }

    fn global_advisories<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: Option<&'a str>,
    ) -> AdvisoryFuture<'a> {
        Box::pin(async move { AdvisoryRead::Failed(self.failure()) })
    }

    fn ghsa<'a>(&'a self, _: Option<(&'a str, &'a str)>, _: &'a str) -> GhsaFuture<'a> {
        Box::pin(async move { GhsaRead::Failed(self.failure()) })
    }

    /// Nothing was asked, with or without a token.
    fn anonymous(&self) -> bool {
        false
    }
}

const GITHUB_HOST: &str = "github.com";

/// A repository by where it lives: its host (lower case, without `www.`), owner
/// and name. Every spelling of one repository, `owner/name`, a URL, a git remote,
/// with or without `.git` or a trailing slash, in any letter case, is equal to
/// the others; GitHub and OSV both match names that way.
#[derive(Clone, Debug)]
pub(super) struct RepositoryId {
    pub host: String,
    pub owner: String,
    pub name: String,
}

impl PartialEq for RepositoryId {
    fn eq(&self, other: &Self) -> bool {
        self.host == other.host
            && self.owner.eq_ignore_ascii_case(&other.owner)
            && self.name.eq_ignore_ascii_case(&other.name)
    }
}

impl Eq for RepositoryId {}

impl RepositoryId {
    /// The repository `text` names: `owner/name` (on github.com), or a URL,
    /// scp-style remote or bare `host/owner/name` of any host. A URL's query and
    /// fragment are not part of the name. `None` when `text` names no repository.
    pub(super) fn of(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        // npm writes a remote as `git+https://…`.
        let trimmed = trimmed.strip_prefix("git+").unwrap_or(trimmed);
        let (host, path, remote) = if let Some((scheme, rest)) = trimmed.split_once("://") {
            if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphabetic()) {
                return None;
            }
            let (authority, path) = rest.split_once('/')?;
            (host_of(authority), path, true)
        } else if let Some((authority, path)) = trimmed
            .split_once(':')
            .filter(|(authority, _)| !authority.contains('/'))
        {
            (host_of(authority), path, true)
        } else {
            match trimmed.split_once('/') {
                Some((first, rest)) if first.contains('.') => (host_of(first), rest, true),
                _ => (GITHUB_HOST.to_owned(), trimmed, false),
            }
        };
        let path = if remote {
            path.split(['?', '#']).next().unwrap_or_default()
        } else {
            path
        };
        let path = path.trim_end_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let (owner, name) = path.rsplit_once('/')?;
        (!host.is_empty() && !owner.is_empty() && !name.is_empty()).then(|| Self {
            host,
            owner: owner.to_owned(),
            name: name.to_owned(),
        })
    }

    pub(super) fn on_github(&self) -> bool {
        self.host == GITHUB_HOST && is_owner(&self.owner) && is_name(&self.name)
    }
}

/// The host of a URL's authority or an scp remote's `user@host`: no user, port or
/// `www.`, lower case.
fn host_of(authority: &str) -> String {
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    let host = host
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    host.strip_prefix("www.")
        .map_or(host.clone(), str::to_owned)
}

/// `owner/name`, or a github.com URL or git remote naming one, read by GitHub's
/// name rules; any other text is refused here, so it never reads as a repository
/// GitHub lacks.
pub(super) fn parse(text: &str) -> Result<(String, String), ArtifactError> {
    match RepositoryId::of(text).filter(RepositoryId::on_github) {
        Some(id) => Ok((id.owner, id.name)),
        None => Err(ArtifactError::new(
            ErrorCode::InvalidInput,
            format!(
                "{text} is not a GitHub repository: use owner/name, a github.com URL or a git remote of GitHub."
            ),
        )),
    }
}

/// `YYYY-MM-DD` of a GitHub timestamp.
pub(super) fn day(timestamp: &str) -> String {
    timestamp.chars().take(10).collect()
}

pub(super) fn caution(contract: &SecurityContract, rule: &str, why: String) -> Option<Value> {
    let rule_facts = contract.rule(rule)?;
    Some(json!({
        "level": match rule_facts.level { CautionLevel::Review => "review", CautionLevel::Note => "note" },
        "rule": rule,
        "basis": match rule_facts.basis { CautionBasis::Reported => "reported", CautionBasis::Heuristic => "heuristic" },
        "why": why,
    }))
}

/// The cautions `read` draws at `now_millis`, with the contract's levels and
/// thresholds. No data draws none: unknown is not clean.
pub(super) fn cautions(
    contract: &SecurityContract,
    read: &RepoRead,
    now_millis: i64,
    today: &str,
) -> Vec<Value> {
    let policy = &contract.policy;
    let mut found = Vec::new();
    let facts = match read {
        RepoRead::Missing => {
            found.extend(caution(
                contract,
                "repositoryMissing",
                format!(
                    "GitHub has no such repository, read {today}. GitHub answers the same for a private repository this credential cannot see."
                ),
            ));
            return found;
        }
        RepoRead::Failed(_) => return found,
        RepoRead::Moved(facts) => {
            found.extend(caution(
                contract,
                "repositoryMoved",
                format!(
                    "GitHub redirects to {}/{}, read {today}.",
                    typed(&facts.owner),
                    typed(&facts.name)
                ),
            ));
            facts
        }
        RepoRead::Found(facts) => facts,
    };
    if facts.archived {
        found.extend(caution(
            contract,
            "archived",
            format!("Archived, read {today}."),
        ));
    }
    if let Some(pushed) = facts
        .pushed_at
        .as_deref()
        .and_then(crate::civil_date::rfc3339_millis)
    {
        let days = (now_millis - pushed).div_euclid(DAY_MILLIS);
        if u64::try_from(days).is_ok_and(|days| days > policy.unmaintained_after_days) {
            found.extend(caution(contract, "unmaintained", format!(
                "Last push {}, {days} days before {today}, over {} (policy unmaintainedAfterDays).",
                day(facts.pushed_at.as_deref().unwrap_or_default()), policy.unmaintained_after_days)));
        }
    }
    if u64::try_from(facts.stars).is_ok_and(|stars| stars < policy.low_traction_stars) {
        found.extend(caution(
            contract,
            "lowTraction",
            format!(
                "{} stars, under {} (policy lowTractionStars), read {today}.",
                facts.stars, policy.low_traction_stars
            ),
        ));
    }
    found
}

pub(super) fn repository_block(facts: &RepoFacts, today: &str) -> Value {
    let mut block = json!({
        "owner": facts.owner, "name": facts.name, "url": facts.url,
        "stars": facts.stars, "forks": facts.forks, "archived": facts.archived,
        "fork": facts.fork, "defaultBranch": facts.default_branch, "observedAt": today,
    });
    if let Some(pushed) = &facts.pushed_at {
        block["pushedAt"] = json!(day(pushed));
    }
    if let Some(license) = &facts.license {
        block["license"] = json!(license);
    }
    block
}

fn sentence(owner: &str, repo: &str, read: &RepoRead, fired: &[Value]) -> String {
    let asked = typed(&format!("{owner}/{repo}"));
    // GitHub's spelling of a repository it found; a moved one is named as asked
    // and as it is now.
    let target = match read {
        RepoRead::Found(facts) => typed(&format!("{}/{}", facts.owner, facts.name)),
        RepoRead::Moved(facts) => format!(
            "{asked} (now {})",
            typed(&format!("{}/{}", facts.owner, facts.name))
        ),
        RepoRead::Missing | RepoRead::Failed(_) => asked,
    };
    let rules = fired
        .iter()
        .filter_map(|c| c["rule"].as_str())
        .collect::<Vec<_>>();
    let tail = if rules.is_empty() {
        " No caution fired; that is not a safety claim.".to_owned()
    } else {
        format!(" Cautions: {}.", rules.join(", "))
    };
    match read {
        RepoRead::Found(f) | RepoRead::Moved(f) => format!(
            "{target}: {} stars, last push {}, {}.{tail}",
            f.stars,
            f.pushed_at.as_deref().map_or("unknown".to_owned(), day),
            if f.archived {
                "archived"
            } else {
                "not archived"
            }
        ),
        RepoRead::Missing => format!(
            "{target}: GitHub has no such repository, or it is private and this credential cannot see it.{tail}"
        ),
        RepoRead::Failed(_) => {
            format!("{target}: GitHub did not answer, so nothing is known of it.")
        }
    }
}

/// GitHub's answer for `owner/name` and the status the row's source reports it
/// as; a call with no GitHub transport says so, never reads as clean.
pub(super) async fn read_github(
    owner: &str,
    name: &str,
    net: &Net<'_>,
) -> (RepoRead, &'static str) {
    let Some(github) = net.github else {
        return (
            RepoRead::Failed(RepoFailure {
                code: ErrorCode::ProviderError,
                why: "No GitHub transport is available to this call.".into(),
            }),
            "unavailable",
        );
    };
    let read = github.read(owner, name).await;
    let status = match &read {
        RepoRead::Found(_) | RepoRead::Moved(_) => "found",
        RepoRead::Missing => "noneKnown",
        RepoRead::Failed(_) => "failed",
    };
    (read, status)
}

/// GitHub's status as a source of advisories once it described the target:
/// `found` only when it holds `held` advisories for it. A repository or
/// commit it described with none is `noneKnown`; the description itself is
/// the row's `repository` block or the entry's `facts`.
pub(super) fn advisory_status(entry: &mut Value, held: usize) {
    if held == 0 && entry["status"] == "found" {
        entry["status"] = json!("noneKnown");
    }
}

/// The `sources[]` entry for a GitHub read, with its failure's code and reason.
pub(super) fn github_source(read: &RepoRead, status: &str, today: &str) -> Value {
    let mut source = json!({"source": "github", "status": status, "observedAt": today});
    if let RepoRead::Failed(failure) = read {
        source["errorCode"] = json!(failure.code);
        source["why"] = json!(failure.why);
    }
    source
}

/// Why OSV's part of the advisories is partial: the ids it did not answer for,
/// and what it said of the first.
fn osv_failure(failed: &[(String, ArtifactError)]) -> String {
    let ids = failed
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "OSV was not read for {ids}: each shows GitHub's record alone, with no OSV merge. {}",
        failed[0].1.message
    )
}

/// The repository row; with `resume`, the advisory walk reads on from the
/// page where an earlier call stopped, and this page lists only what it reads.
pub(super) async fn run(
    repo: &str,
    resume: Option<&str>,
    brief: Map<String, Value>,
    view: SecurityCheckView,
    net: &Net<'_>,
) -> Result<Value, ArtifactError> {
    let (owner, name) = parse(repo)?;
    let from = resume.map(advisories::SavedWalk::parse).transpose()?;
    let contract =
        security_contract().map_err(|why| ArtifactError::new(ErrorCode::ProviderError, why))?;
    let now = crate::civil_date::unix_millis();
    let today = crate::civil_date::date_millis(now);
    let (read, status) = read_github(&owner, &name, net).await;
    let fired = cautions(contract, &read, now, &today);
    let mut data = json!({ "answer": sentence(&owner, &name, &read, &fired) });
    let mut source = github_source(&read, status, &today);
    let mut failure = match &read {
        RepoRead::Failed(failure) => Some(failure.clone()),
        _ => None,
    };
    let mut osv_part = None;
    let mut reads_on = None;
    if let RepoRead::Found(facts) | RepoRead::Moved(facts) = &read {
        data["repository"] = repository_block(facts, &today);
        let listed = advisories::list(
            &owner,
            &name,
            facts.visibility,
            net,
            &Selection::All,
            from.as_ref(),
        )
        .await?;
        reads_on = listed.resume.clone();
        if listed.osv_skipped {
            let reason = if facts.visibility == Visibility::Unknown {
                "GitHub did not say whether the repository is private (its answer has neither `private` nor `visibility`), so it is treated as private and its advisory ids are not sent to a third party"
            } else {
                "the repository is private, and its advisory ids are not sent to a third party"
            };
            paging::push_warnings(
                &mut data,
                [format!(
                    "OSV was not asked about the advisories of {}: {reason}. Each advisory shows GitHub's record alone, with no OSV merge.",
                    typed(&format!("{}/{}", facts.owner, facts.name))
                )],
            );
        }
        osv_part = listed
            .osv_status
            .map(|status| json!({"source": "osv", "status": status}));
        // Each advisory OSV keeps no record of: GitHub's own, read whole with
        // `repo`. Only asked of a repository GitHub calls public, as OSV hears
        // the id.
        if !listed.osv_lacks.is_empty() {
            let mut with_repo = brief.clone();
            with_repo.insert(
                "repo".into(),
                json!(format!("{}/{}", facts.owner, facts.name)),
            );
            let ids = listed
                .osv_lacks
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            super::answer::advisory_calls(&mut data, "readWhole", &ids, &with_repo);
        }
        if !listed.osv_failed.is_empty() {
            let why = osv_failure(&listed.osv_failed);
            // `osv_part` below states why on the OSV entry.
            super::source::source_failed(&mut data, &[]);
            osv_part = Some(json!({
                "source": "osv",
                "status": "partial",
                "errorCode": listed.osv_failed[0].1.code,
                "why": why,
            }));
        }
        let (rows, stopped) = (listed.advisories, advisories::failure(&listed.end));
        let read = rows.len();
        // A withdrawn advisory is listed with its date and never counted.
        let withdrawn = rows
            .iter()
            .filter(|row| row.get("withdrawn").is_some())
            .count();
        advisory_status(&mut source, read - withdrawn);
        if read > 0 || stopped.is_none() {
            let mut clause = if stopped.is_none() {
                advisories::count_clause(read - withdrawn)
            } else {
                advisories::partial_count_clause(read - withdrawn)
            };
            if withdrawn > 0 {
                clause.push_str(&format!("; {withdrawn} withdrawn, not counted"));
            }
            data["answer"] = json!(format!(
                "{} {clause}.",
                data["answer"].as_str().unwrap_or_default()
            ));
            data["advisories"] = Value::Array(rows);
        }
        if let Some(stopped) = stopped {
            source["status"] = json!("partial");
            source["errorCode"] = json!(stopped.code);
            let mut why = advisories::unread_why(read, &stopped, reads_on.is_some());
            if read > 0 {
                why.push_str(" Each row read shows GitHub's record alone, with no OSV merge.");
            }
            source["why"] = json!(why);
            failure = Some(stopped);
        }
    }
    if !fired.is_empty() {
        data["cautions"] = Value::Array(fired);
    }
    data["sources"] = json!([source]);
    if let (Some(part), Some(sources)) = (osv_part, data["sources"].as_array_mut()) {
        sources.push(part);
    }
    if let Some(failure) = &failure {
        super::source::source_failed(&mut data, std::slice::from_ref(&failure.why));
    }
    if from.is_some() {
        // The facts and cautions were stated by the page that began the walk.
        if let Some(row) = data.as_object_mut() {
            for repeated in ["repository", "cautions"] {
                row.remove(repeated);
            }
        }
        paging::push_warnings(
            &mut data,
            ["Repository facts and cautions are on the first page of this walk; this page continues the advisory list only.".to_owned()],
        );
    }
    if let Some(saved) = reads_on {
        let mut next = brief;
        next.insert("operation".into(), json!("repository"));
        next.insert("repo".into(), json!(repo));
        super::summary::carry(view, &mut next);
        next.insert("resume".into(), json!(saved.token()));
        data["calls"]["nextPage"] =
            Continuation::continued(ToolId::SecurityCheck, Value::Object(next)).build();
        paging::mark_partial(
            &mut data,
            PartialReason::ProviderIncompleteResults,
            &["The advisory list stopped at a page GitHub did not answer; next.nextPage reads on from there.".to_owned()],
        );
    }
    crate::response::result::finish_calls(&mut data);
    Ok(in_view(view, repo, data))
}

/// The finished row as `view` states it.
fn in_view(view: SecurityCheckView, repo: &str, data: Value) -> Value {
    let mut query = Map::new();
    query.insert("operation".into(), json!("repository"));
    query.insert("repo".into(), json!(repo));
    super::summary::apply(
        view,
        data,
        super::summary::Target {
            query,
            asked_name: None,
            asked_version: None,
            unfixed_in_major: 0,
            // A repository row counts and cautions; it never says `affected`.
            gives_verdict: false,
        },
    )
}

#[cfg(test)]
#[path = "repository_tests.rs"]
mod tests;
