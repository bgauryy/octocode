//! A repository's published security advisories as GitHub lists them: every
//! page walked once, each advisory read as an OSV-shaped record so one parser
//! ([`record::advisory`]) states it, and merged with OSV's record of the same
//! advisory by id or alias. A monorepo's advisory names several packages: the
//! row's own package keeps its entries and the others move to `alsoAffects`.
use super::Net;
use super::osv::CONCURRENT_READS;
use super::record::{self, SecurityAdvisory, Selection};
use super::repository::{RepoFailure, RepoRead, RepositoryRead, Visibility};
use super::source::{self, Target};
use crate::artifact::ArtifactError;
use futures_util::{StreamExt as _, stream};
use octocode_contract_types::ErrorCode;
use octocode_github::{AdvisoryPage, RepositoryAdvisory};
use octocode_runtime_base::contracts::security::{SecurityContract, security_contract};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::{future::Future, pin::Pin};

/// One page of a repository's advisories, or why there is none.
#[derive(Clone, Debug)]
pub enum AdvisoryRead {
    Page(AdvisoryPage),
    /// GitHub has no such repository (deleted, private, or never there).
    Missing,
    Failed(RepoFailure),
}

pub type AdvisoryFuture<'a> = Pin<Box<dyn Future<Output = AdvisoryRead> + Send + 'a>>;

/// One GitHub advisory read by its GHSA id, or why there is none.
#[derive(Clone, Debug)]
pub enum GhsaRead {
    Found(Box<RepositoryAdvisory>),
    /// GitHub has no such advisory there.
    Missing,
    Failed(RepoFailure),
}

pub type GhsaFuture<'a> = Pin<Box<dyn Future<Output = GhsaRead> + Send + 'a>>;

/// How a walk ended.
#[derive(Debug)]
pub(super) enum End {
    /// GitHub linked no further page.
    Complete,
    Missing,
    /// A page was refused or malformed: what was read stays in
    /// [`Walked::advisories`], and the list is not whole.
    Failed(RepoFailure),
    /// Not walked: the list is compared with OSV's records of a commit, and
    /// OSV's walk had not reached its last page.
    Deferred,
}

#[derive(Debug)]
pub(super) struct Walked {
    pub advisories: Vec<RepositoryAdvisory>,
    pub end: End,
    /// Where a walk that stopped after reading a page reads on.
    pub resume: Option<SavedWalk>,
}

/// Where a stopped advisory walk reads on: GitHub's cursor of the page that
/// failed, and every cursor asked, so a cycle across calls still fails. The
/// pages before it were listed by the calls that read them.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct SavedWalk {
    after: String,
    asked: BTreeSet<String>,
}

impl SavedWalk {
    /// The saved walk a `resume` names; only this tool issues one.
    pub(super) fn parse(token: &str) -> Result<Self, ArtifactError> {
        serde_json::from_str(token).map_err(|_| {
            ArtifactError::new(ErrorCode::InvalidInput, "Invalid saved advisory walk.")
        })
    }

    pub(super) fn token(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Every page of `owner/repo`'s advisories from the start, or from where
/// `from` stopped, each cursor asked once: a page that names a cursor already
/// asked is a cycle, which fails the walk rather than repeating its
/// advisories. A page that fails after a page was read, by this call or by
/// the call `from` continues, saves where to read on; the first page of a
/// fresh walk does not, as asking again is the same call.
pub(super) async fn walk(
    owner: &str,
    repo: &str,
    github: &dyn RepositoryRead,
    from: Option<&SavedWalk>,
) -> Walked {
    walk_pages(
        |after: Option<String>| async move { github.advisories(owner, repo, after.as_deref()).await },
        from,
    )
    .await
}

/// Every page `ask` answers, from the start or from where `from` stopped,
/// each cursor asked once ([`walk`]).
pub(super) async fn walk_pages<F: Future<Output = AdvisoryRead>>(
    ask: impl Fn(Option<String>) -> F,
    from: Option<&SavedWalk>,
) -> Walked {
    let mut advisories = Vec::new();
    let mut asked = from.map(|saved| saved.asked.clone()).unwrap_or_default();
    let mut after = from.map(|saved| saved.after.clone());
    let mut advanced = from.is_some();
    let stop = |advisories, end, after: Option<String>, asked, advanced| Walked {
        advisories,
        resume: match (&end, after) {
            (End::Failed(_), Some(after)) if advanced => Some(SavedWalk { after, asked }),
            _ => None,
        },
        end,
    };
    loop {
        let page = match ask(after.clone()).await {
            AdvisoryRead::Page(page) => page,
            AdvisoryRead::Missing => return stop(advisories, End::Missing, None, asked, false),
            AdvisoryRead::Failed(failure) => {
                return stop(advisories, End::Failed(failure), after, asked, advanced);
            }
        };
        advanced = true;
        advisories.extend(page.advisories);
        let Some(next) = page.next else {
            return stop(advisories, End::Complete, None, asked, false);
        };
        if !asked.insert(next.clone()) {
            let cycle = End::Failed(RepoFailure {
                code: ErrorCode::Decode,
                why: "GitHub linked the same advisory page twice, so the list stopped there."
                    .into(),
            });
            return stop(advisories, cycle, None, asked, false);
        }
        after = Some(next);
    }
}

/// The advisory in OSV's record shape. A vulnerable range GitHub writes as
/// text stays as written under `ecosystem_specific`: no range is inferred from
/// it.
fn osv_shape(advisory: &RepositoryAdvisory, contract: &SecurityContract) -> Value {
    let affected = advisory
        .vulnerabilities
        .iter()
        .map(|entry| {
            let mut row = json!({});
            if let Some(package) = &entry.package {
                // An ecosystem the contract does not map stays as GitHub
                // wrote it; a mapped one is named as OSV's records name it,
                // the form every record here is compared in.
                let ecosystem = package.ecosystem.as_deref().map(|name| {
                    contract
                        .ecosystem_of_github(name)
                        .map_or(name, |input| contract.osv_name_of(input))
                        .to_owned()
                });
                row["package"] = json!({
                    "ecosystem": ecosystem,
                    "name": package.name,
                });
            }
            row["ecosystem_specific"] = json!({
                "vulnerable_version_range": entry.vulnerable_version_range,
                "patched_versions": entry.patched_versions,
            });
            if let Some(first) = &entry.first_patched_version {
                row["ecosystem_specific"]["first_patched_version"] = json!(first);
            }
            row
        })
        .collect::<Vec<_>>();
    let mut record = json!({
        "id": advisory.ghsa_id,
        "aliases": advisory.cve_id.iter().collect::<Vec<_>>(),
        "summary": advisory.summary,
        "details": advisory.description,
        "published": advisory.published_at,
        "modified": advisory.updated_at,
        "withdrawn": advisory.withdrawn_at,
        "affected": affected,
        "database_specific": {
            "severity": advisory.severity,
            "source": advisory.html_url,
            "cwe_ids": advisory.cwe_list(),
        },
    });
    let mut references = advisory
        .references
        .iter()
        // GitHub states no type for these: none is invented.
        .map(|url| json!({"url": url}))
        .collect::<Vec<_>>();
    // The record's own API address: where GitHub serves it whole. [`row`]
    // leaves it out when the row's link is its page.
    if let Some(url) = &advisory.url {
        references.push(json!({"type": "ADVISORY", "url": url}));
    }
    if !references.is_empty() {
        record["references"] = json!(references);
    }
    record
}

/// What GitHub states of a record beyond its row: its class in the global
/// database when the row's `githubType` cannot state it. `reviewed`,
/// `unreviewed` and `malware` are `githubType` values (and `malware`, on a
/// package row, the contract's `malware` caution), so they take no note
/// here; a record the row does not list gets [`unreviewed_note`] from its
/// caller. Another class is named as GitHub writes it when it is a plain word.
pub(super) fn github_notes(advisory: &RepositoryAdvisory) -> Vec<String> {
    let id = super::answer::stated_id(&advisory.ghsa_id);
    match advisory.kind.as_deref() {
        None | Some("reviewed" | "unreviewed" | "malware") => Vec::new(),
        Some(other) if other.chars().all(|c| c.is_ascii_alphabetic()) => {
            vec![format!("GitHub's database classes {id} as {other}.")]
        }
        Some(_) => vec![format!(
            "GitHub's database classes {id} in a class that is not a plain word."
        )],
    }
}

/// What GitHub's `unreviewed` class says of a record the row does not list,
/// by its stated id.
pub(super) fn unreviewed_note(id: &str) -> String {
    format!("GitHub has not reviewed {id}: its database marks it unreviewed.")
}

/// GitHub's own statements beside OSV's shape: each CVSS vector with the
/// score GitHub publishes for it, EPSS, the publisher and the credits.
fn github_facts(row: &mut SecurityAdvisory, advisory: &RepositoryAdvisory) {
    row.cvss = advisory
        .cvss_all()
        .into_iter()
        .map(|(vector, score)| {
            let mut cvss = json!({"type": record::cvss_type(&vector), "vector": vector});
            if let Some(score) = score {
                cvss["score"] = json!(score);
            }
            cvss
        })
        .collect();
    // Only a class the contract names; another stays in `github_notes`.
    row.github_type = advisory
        .kind
        .clone()
        .filter(|kind| matches!(kind.as_str(), "reviewed" | "unreviewed" | "malware"));
    row.epss = advisory
        .epss
        .map(|epss| json!({"percentage": epss.percentage, "percentile": epss.percentile}));
    row.publisher = advisory
        .publisher
        .as_ref()
        .and_then(|user| user.login.clone());
    row.credits = advisory
        .credit_logins()
        .into_iter()
        .map(|(login, kind)| {
            let mut credit = json!({"login": login});
            if let Some(kind) = kind {
                credit["type"] = json!(kind);
            }
            credit
        })
        .collect();
}

/// The comparators a GitHub version text names, in order, as `(operator,
/// version)`; a bare version has no operator. Maintainers write it free hand:
/// a list (`2.0.0, 1.5.3`) or ranges (`>=6.2.6, >=6.1.5 <6.2.0`, `>= 8.3.0, <=
/// 8.3.2`, with or without a space after the operator).
pub(super) fn comparators(text: &str) -> Vec<(&str, &str)> {
    let is_operator = |token: &str| token.chars().all(|c| matches!(c, '<' | '>' | '='));
    let mut found = Vec::new();
    let mut tokens = text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|token| !token.is_empty())
        .peekable();
    while let Some(token) = tokens.next() {
        let at = token.len() - token.trim_start_matches(['<', '>', '=']).len();
        let (operator, mut version) = token.split_at(at);
        // `>= 6.2.6`: the operator stands apart from its version.
        if version.is_empty()
            && let Some(next) = tokens.next_if(|next| !is_operator(next))
        {
            version = next;
        }
        if !version.is_empty() {
            found.push((operator, version));
        }
    }
    found
}

/// The fixed versions one patched-versions text names: a bare version, or one
/// after `=` or `>=`, is a fix; an upper bound (`<6.2.0`) says where a fix's
/// release line ends, and any other comparator names no version that is fixed.
/// The text itself stays whole on the `advisory` row (`ecosystemSpecific`).
pub(super) fn fixes(text: &str) -> Vec<&str> {
    comparators(text)
        .into_iter()
        .filter(|(operator, _)| matches!(*operator, "" | "=" | ">="))
        .map(|(_, version)| version)
        .collect()
}

/// The fixed versions GitHub states, each once and as a version: `6.2.6` for
/// `>=6.2.6` as for `6.2.6`, the form OSV's `fixed` events use, so one fix
/// reads the same from either source.
fn patched(advisory: &RepositoryAdvisory, own_package: impl Fn(&str) -> bool) -> Vec<String> {
    let mut versions: Vec<String> = Vec::new();
    for entry in &advisory.vulnerabilities {
        let named = entry
            .package
            .as_ref()
            .and_then(|package| package.name.as_deref());
        if !named.is_some_and(&own_package) {
            continue;
        }
        for version in entry
            .patched_versions
            .iter()
            .chain(&entry.first_patched_version)
            .flat_map(|text| fixes(text))
        {
            if !versions.iter().any(|known| known == version) {
                versions.push(version.to_owned());
            }
        }
    }
    versions
}

/// The advisory as a row of `selection`'s package; with every entry when the
/// selection is [`Selection::All`].
pub(super) fn row(
    advisory: &RepositoryAdvisory,
    selection: &Selection<'_>,
) -> Result<SecurityAdvisory, String> {
    let contract = security_contract().map_err(str::to_owned)?;
    let mut row = record::advisory(&osv_shape(advisory, contract), selection)?;
    row.link = advisory
        .html_url
        .clone()
        .unwrap_or_else(|| format!("https://github.com/advisories/{}", advisory.ghsa_id));
    row.database = Some(
        if row.link.starts_with("https://github.com/advisories/") {
            record::GITHUB_DATABASE
        } else {
            record::GITHUB_REPOSITORY_DATABASE
        }
        .to_owned(),
    );
    // GitHub's page is both the link and the database's file: stated once.
    if row.source.as_deref() == Some(row.link.as_str()) {
        row.source = None;
    }
    // The record's API address is its link's: the link states it.
    if let Some(api) = api_address(&row.link) {
        row.references
            .retain(|reference| reference.get("url").and_then(Value::as_str) != Some(&api));
    }
    let own = row
        .affected
        .iter()
        .filter_map(|entry| entry["packageName"].as_str())
        .collect::<BTreeSet<_>>();
    // Fixes of several packages are told apart by the row's `affected` entries.
    if own.len() <= 1 {
        row.fixed_in = patched(advisory, |name| own.contains(name));
    }
    github_facts(&mut row, advisory);
    Ok(row)
}

/// The API address GitHub serves the advisory of page `link` at: a global
/// advisory (`github.com/advisories/GHSA-…`) or a repository's
/// (`github.com/<owner>/<repo>/security/advisories/GHSA-…`).
fn api_address(link: &str) -> Option<String> {
    let path = link.strip_prefix("https://github.com/")?;
    let parts = path.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["advisories", id] => Some(format!("https://api.github.com/advisories/{id}")),
        [owner, repo, "security", "advisories", id] => Some(format!(
            "https://api.github.com/repos/{owner}/{repo}/security-advisories/{id}"
        )),
        _ => None,
    }
}

/// The rows of GitHub's advisories among `rows` that share an id or alias with
/// one of `known` (OSV's records): the twin of an advisory OSV listed.
pub(super) fn twins(
    known: &[&SecurityAdvisory],
    rows: Vec<SecurityAdvisory>,
) -> Vec<SecurityAdvisory> {
    let ids = |row: &SecurityAdvisory| {
        std::iter::once(row.id.clone())
            .chain(row.aliases.iter().cloned())
            .collect::<BTreeSet<_>>()
    };
    let known_ids = known
        .iter()
        .flat_map(|row| ids(row))
        .collect::<BTreeSet<_>>();
    rows.into_iter()
        .filter(|row| ids(row).iter().any(|id| known_ids.contains(id)))
        .collect()
}

/// Why a walk's list is not whole; none: it is.
pub(super) fn failure(end: &End) -> Option<RepoFailure> {
    match end {
        End::Complete | End::Deferred => None,
        End::Missing => Some(RepoFailure {
            code: ErrorCode::NotFound,
            why: "GitHub has no advisory list for this repository.".into(),
        }),
        End::Failed(failure) => Some(failure.clone()),
    }
}

/// The sentence's clause for a finished list.
pub(super) fn count_clause(count: usize) -> String {
    match count {
        0 => "No published repository advisory".to_owned(),
        1 => "1 published repository advisory".to_owned(),
        _ => format!("{count} published repository advisories"),
    }
}

/// The sentence's clause for a list that stopped before its end.
pub(super) fn partial_count_clause(count: usize) -> String {
    format!(
        "{} read before GitHub stopped, so the list is not whole",
        count_clause(count)
    )
}

/// Why the GitHub source is partial: what of `subject` (a list of
/// advisories) the page that stopped it left unread, with its failure
/// ([`super::source::unread_because`]). A walk saved to read on says so; else
/// a new call reads the list again from the start.
pub(super) fn unread_why(
    subject: &str,
    read: usize,
    stopped: &RepoFailure,
    reads_on: bool,
) -> String {
    if read == 0 {
        return super::source::unread_because(&format!("{subject} not read"), stopped);
    }
    let then = if reads_on {
        "The reply's next reads on from the page that failed."
    } else {
        "A new call reads the list again from the start."
    };
    let unread = format!("{subject} read in part: {read} listed, the rest not read");
    format!("{} {then}", super::source::unread_because(&unread, stopped))
}

/// GitHub's advisories of a repository as rows of `selection`, and how the walk
/// ended. A record that does not read fails the call whole, so none is dropped.
pub(super) async fn github_rows(
    owner: &str,
    repo: &str,
    net: &Net<'_>,
    selection: &Selection<'_>,
    from: Option<&SavedWalk>,
) -> Result<(Vec<SecurityAdvisory>, End, Option<SavedWalk>), ArtifactError> {
    let Some(github) = net.github else {
        let failure = RepoFailure {
            code: ErrorCode::ProviderError,
            why: "No GitHub transport is available to this call.".into(),
        };
        return Ok((Vec::new(), End::Failed(failure), None));
    };
    let walked = walk(owner, repo, github, from).await;
    let target = format!("{owner}/{repo}");
    let rows = walked
        .advisories
        .iter()
        .map(|advisory| {
            row(advisory, selection).map_err(|reason| super::osv::malformed(&reason, &target))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((rows, walked.end, walked.resume))
}

/// What a repository's advisories read as.
pub(super) struct Listed {
    /// One brief row per advisory read, OSV's record merged with GitHub's by
    /// id or alias. When the walk did not finish these are the rows of the
    /// pages already read, GitHub's alone and not the whole list: `end` says so.
    pub advisories: Vec<Value>,
    pub end: End,
    /// The repository is not known to be public: its advisory ids were not sent
    /// to OSV, so each row stands on GitHub's record alone.
    pub osv_skipped: bool,
    /// Advisory ids OSV failed to answer for, with why: their rows stand on
    /// GitHub's record alone.
    pub osv_failed: Vec<(String, ArtifactError)>,
    /// What OSV said when it was asked for at least one id: `found` when it
    /// kept a record of one, `noneKnown` when it kept none; `None`: not asked.
    pub osv_status: Option<&'static str>,
    /// Advisory ids OSV was asked for and keeps no record of: GitHub's record
    /// is the only one, read whole by an `advisory` call with `repo`.
    pub osv_lacks: Vec<String>,
    /// Where a walk that stopped reads on.
    pub resume: Option<SavedWalk>,
}

/// The repository's advisories, each merged with OSV's record of the same
/// advisory, asked together. An advisory OSV does not list stands on GitHub's
/// record; so does one OSV failed to answer for, named in `osv_failed`. A
/// repository GitHub does not say is public never has its advisory ids sent to
/// OSV: the rows are GitHub's alone, and `osv_skipped` says so. A list that
/// stopped early keeps the rows read, GitHub's alone, and asks OSV nothing.
pub(super) async fn list(
    owner: &str,
    repo: &str,
    visibility: Visibility,
    net: &Net<'_>,
    selection: &Selection<'_>,
    from: Option<&SavedWalk>,
) -> Result<Listed, ArtifactError> {
    let (github, end, resume) = github_rows(owner, repo, net, selection, from).await?;
    let osv_skipped = !visibility.third_party_may_hear() && !github.is_empty();
    let ask_osv = matches!(end, End::Complete) && !osv_skipped;
    let asked = github.iter().filter(|_| ask_osv).map(|row| &row.id);
    let answers = stream::iter(asked)
        .map(|id| async move {
            let answer = source::ask_all(&Target::Advisory(id.clone()), net).await;
            (id, answer)
        })
        .buffered(CONCURRENT_READS)
        .collect::<Vec<_>>()
        .await;
    let mut osv = Vec::new();
    let mut osv_failed = Vec::new();
    let mut osv_lacks = Vec::new();
    for (id, answer) in answers {
        match answer {
            Ok(results) => osv.extend(results.into_iter().flat_map(|result| result.advisories)),
            Err(error) if error.code == ErrorCode::NotFound => osv_lacks.push(id.clone()),
            Err(error) => osv_failed.push((id.clone(), error)),
        }
    }
    let osv_status = (ask_osv && !github.is_empty()).then_some(if osv.is_empty() {
        "noneKnown"
    } else {
        "found"
    });
    let rows = osv.iter().chain(&github).collect::<Vec<_>>();
    let mut advisories = record::advisory_groups(&rows)
        .iter()
        .map(|group| record::group_brief(group))
        .collect::<Vec<_>>();
    record::most_severe_first(&mut advisories);
    Ok(Listed {
        advisories,
        end,
        osv_skipped,
        osv_failed,
        osv_status,
        osv_lacks,
        resume,
    })
}

/// What GitHub said of one GHSA id: its records, and each read that failed.
pub(super) struct GhsaLookup {
    /// GitHub was asked: the id is a GHSA and a GitHub transport exists.
    asked: bool,
    rows: Vec<SecurityAdvisory>,
    /// Each read that failed, named by where it was asked.
    failed: Vec<(String, RepoFailure)>,
    warnings: Vec<String>,
    /// `repo` named a repository GitHub does not have (or hides from this
    /// read): its own advisories were not read, which is no absence.
    repo_missing: Option<String>,
}

/// The GHSA id `id` on GitHub: the global database's record and, with `repo`,
/// the repository's own (one GitHub has not reviewed into its database is
/// found only there). Any other id, or no GitHub transport, asks nothing.
pub(super) async fn ghsa(id: &str, repo: Option<(&str, &str)>, net: &Net<'_>) -> GhsaLookup {
    let mut lookup = GhsaLookup {
        asked: false,
        rows: Vec::new(),
        failed: Vec::new(),
        warnings: Vec::new(),
        repo_missing: None,
    };
    let is_ghsa = id
        .split_once('-')
        .is_some_and(|(prefix, _)| prefix.eq_ignore_ascii_case("GHSA"));
    let repo = match repo {
        Some(named) if is_ghsa => Some(named),
        Some(_) => {
            lookup.warnings.push(format!(
                "owner and repo are read for a GHSA id only; {} is not one, so no repository advisory was read.",
                super::answer::typed(id)
            ));
            None
        }
        None => None,
    };
    let Some(github) = net.github.filter(|_| is_ghsa) else {
        return lookup;
    };
    lookup.asked = true;
    let own = repo;
    let (global, own) = futures_util::join!(github.ghsa(None, id), async {
        match own {
            Some(at) => Some((
                format!("{}/{}", at.0, at.1),
                github.ghsa(Some(at), id).await,
            )),
            None => None,
        }
    });
    // GitHub answers 404 for a missing repository as for a missing advisory:
    // the repository read tells them apart.
    let own_missing = matches!(own, Some((_, GhsaRead::Missing)));
    let reads = std::iter::once(("GitHub's advisory database".to_owned(), global)).chain(own);
    for (place, read) in reads {
        match read {
            GhsaRead::Found(advisory) => match row(&advisory, &Selection::All) {
                Ok(found) => {
                    lookup.warnings.extend(github_notes(&advisory));
                    lookup.rows.push(found);
                }
                Err(reason) => lookup.failed.push((
                    place,
                    RepoFailure {
                        code: ErrorCode::Decode,
                        why: format!("GitHub answered with {reason}."),
                    },
                )),
            },
            GhsaRead::Missing => {}
            GhsaRead::Failed(failure) => lookup.failed.push((place, failure)),
        }
    }
    if let (true, Some((owner, name))) = (own_missing, &repo) {
        let place = format!("{owner}/{name}");
        match github.read(owner, name).await {
            RepoRead::Missing => {
                let missing = format!(
                    "GitHub has no repository {} (or hides it from this read), so its own advisories were not read.",
                    super::answer::typed(&place)
                );
                lookup.warnings.push(missing.clone());
                lookup.repo_missing = Some(missing);
            }
            RepoRead::Failed(failure) => lookup.failed.push((place, failure)),
            RepoRead::Found(_) | RepoRead::Moved(_) => {}
        }
    }
    lookup
}

impl GhsaLookup {
    pub(super) fn found(&self) -> bool {
        !self.rows.is_empty()
    }

    /// What OSV's "no record" adds when GitHub has none either.
    pub(super) fn none_clause(&self) -> Option<String> {
        (self.asked && self.rows.is_empty() && self.failed.is_empty()).then(|| {
            match &self.repo_missing {
                Some(missing) => {
                    format!("GitHub's advisory database has no such advisory either. {missing}")
                }
                None => "GitHub has no such advisory either.".to_owned(),
            }
        })
    }

    /// What a failure adds when GitHub found nothing because a read failed:
    /// each read not made, so not-found never stands for not-checked.
    pub(super) fn failed_clause(&self) -> Option<String> {
        (self.asked && self.rows.is_empty() && !self.failed.is_empty()).then(|| {
            let unread = self
                .failed
                .iter()
                .map(|(place, failure)| format!("{place} was not read: {}", failure.why))
                .collect::<Vec<_>>();
            format!("{} Whether GitHub has it is unknown.", unread.join(" "))
        })
    }

    /// GitHub's records as a source, once it answered.
    pub(super) fn result(&self) -> Option<source::SourceResult> {
        (self.asked && (self.found() || self.failed.is_empty())).then(|| {
            let mut result = source::SourceResult::new(source::Source::GitHub);
            result.advisories = self.rows.clone();
            result
        })
    }

    /// Each failed read: the GitHub entry says so when nothing was found, and
    /// the row is partial either way, never clean.
    pub(super) fn apply(&self, data: &mut Value) {
        if !self.failed.is_empty() {
            let why = self
                .failed
                .iter()
                .map(|(place, failure)| {
                    super::source::unread_because(&format!("{place} was not read"), failure)
                })
                .collect::<Vec<_>>();
            for (_, failure) in &self.failed {
                super::source::state_refusal(data, failure);
            }
            // The one GitHub entry names every read that failed, with its
            // place: `failed` when nothing was found, else `partial`.
            if !data["sources"].is_array() {
                data["sources"] = json!([]);
            }
            if let Some(sources) = data["sources"].as_array_mut() {
                let at = match sources.iter().position(|entry| entry["source"] == "github") {
                    Some(at) => at,
                    None => {
                        sources.push(json!({"source": "github", "status": "failed"}));
                        sources.len() - 1
                    }
                };
                for ((_, failure), why) in self.failed.iter().zip(&why) {
                    super::source::note_failure(&mut sources[at], failure.code, why);
                }
            }
            super::source::source_failed(data, &why);
        }
        if !self.warnings.is_empty() {
            octocode_runtime_base::response::paging::push_warnings(data, self.warnings.clone());
        }
    }
}

#[cfg(test)]
#[path = "advisories_tests.rs"]
mod tests;
