//! Trust signals of a package row: what the registry says of the version
//! (deprecated, yanked, its source repository) and what GitHub says of that
//! repository. Facts and cautions, never a verdict; a GitHub that did not answer
//! is disclosed with its reason and leaves the OSV answer standing.
use super::Net;
use super::advisories::{self, End};
use super::answer::typed;
use super::record::{SecurityAdvisory, Selection};
use super::repository::{
    self, RepoRead, caution, github_source, read_github, repository_block,
};
use super::source::{Confirmed, Source, SourceResult};
use super::update::Latest;
use crate::contracts::security::{SecurityContract, security_contract};
use crate::id::ToolId;
use crate::providers::artifact::ArtifactItem;
use crate::response::paging;
use crate::response::result::Continuation;
use serde_json::{Map, Value, json};

/// What the registry states of the confirmed version.
#[derive(Clone, Debug, Default)]
pub(super) struct Registered {
    pub repository: Option<String>,
    pub deprecated: Option<String>,
    pub yanked: bool,
    pub yanked_reason: Option<String>,
}

impl Registered {
    pub(super) fn of(artifact: &ArtifactItem) -> Self {
        Self {
            repository: artifact
                .repository
                .clone()
                .filter(|url| !url.trim().is_empty()),
            deprecated: artifact.deprecated.clone(),
            yanked: artifact.yanked == Some(true),
            yanked_reason: artifact.yanked_reason.clone(),
        }
    }
}

/// Where the source repository stands after asking.
enum Repo {
    /// The registry names none.
    Unnamed,
    /// It names a repository GitHub does not host.
    Elsewhere(String),
    Github {
        read: RepoRead,
        status: &'static str,
    },
}

pub(super) struct Trust {
    registered: Registered,
    repo: Repo,
    /// The repository the latest release names, read because GitHub lacks
    /// the one this version names.
    successor: Option<Successor>,
    /// The source repository's advisories about this package, once GitHub found it.
    advisories: Option<Listing>,
    /// The first page states every trust fact; the last page of a resumed walk
    /// states only the advisory comparison, as the first page said the rest.
    first_page: bool,
}

/// The repository the registry's latest release names, when GitHub lacks the
/// one the version names and the two differ.
struct Successor {
    /// `owner/name` the version names.
    from: String,
    /// `owner/name` the latest release names.
    to: String,
    /// The latest release.
    version: String,
    read: RepoRead,
    status: &'static str,
}

/// The source repository's advisories that name the package: a monorepo's
/// advisory about another of its packages only is not this package's.
struct Listing {
    rows: Vec<SecurityAdvisory>,
    end: End,
    /// Compared with OSV's records: false until OSV's walk is whole.
    compared: bool,
    /// OSV did not answer, so there was nothing to compare them with.
    osv_unanswered: bool,
    /// Ids of those OSV does not list at this version: the repository's, not
    /// known to affect it.
    others: Vec<String>,
}

pub(super) async fn read(registered: Registered, confirmed: &Confirmed, net: &Net<'_>) -> Trust {
    let mut advisories = None;
    let repo = match registered.repository.as_deref() {
        None => Repo::Unnamed,
        Some(url) => match repository::parse(url) {
            Err(_) => Repo::Elsewhere(url.to_owned()),
            Ok((owner, name)) => {
                let (read, status) = read_github(&owner, &name, net).await;
                if matches!(read, RepoRead::Found(_) | RepoRead::Moved(_)) {
                    advisories = Some(listing(&owner, &name, confirmed, net).await);
                }
                Repo::Github { read, status }
            }
        },
    };
    Trust {
        registered,
        repo,
        successor: None,
        advisories,
        first_page: true,
    }
}

/// The advisories of `owner/name` as rows of the confirmed package; a record
/// that does not read stops the list with its reason.
async fn listing(owner: &str, name: &str, confirmed: &Confirmed, net: &Net<'_>) -> Listing {
    let names = [confirmed.name.clone()];
    let selection = Selection::Named(confirmed.ecosystem, &names, confirmed.release.as_deref());
    let (mut rows, end) = match advisories::github_rows(owner, name, net, &selection, None).await {
        Ok((rows, end, _)) => (rows, end),
        Err(error) => (
            Vec::new(),
            End::Failed(repository::RepoFailure {
                code: error.code,
                why: error.message,
            }),
        ),
    };
    rows.retain(|row| !row.affected.is_empty());
    Listing {
        rows,
        end,
        compared: false,
        osv_unanswered: false,
        others: Vec::new(),
    }
}

fn registry_cautions(contract: &SecurityContract, registered: &Registered) -> Vec<Value> {
    let mut found = Vec::new();
    if let Some(message) = &registered.deprecated {
        found.extend(caution(
            contract,
            "deprecated",
            format!(
                "The registry marks this version deprecated: {}",
                message.trim()
            ),
        ));
    }
    if registered.yanked {
        let reason = registered
            .yanked_reason
            .as_deref()
            .map_or(String::new(), |reason| format!(": {}", reason.trim()));
        found.extend(caution(
            contract,
            "yanked",
            format!("The registry yanked this version{reason}."),
        ));
    }
    if registered.repository.is_none() {
        found.extend(caution(
            contract,
            "noSourceRepo",
            "The registry names no source repository for this version.".to_owned(),
        ));
    }
    found
}

impl Trust {
    /// The trust read of the last page of a resumed walk: it states the
    /// repository's advisories compared with OSV's, and nothing the first page
    /// already stated.
    pub(super) fn on_last_page(mut self) -> Self {
        self.first_page = false;
        self
    }

    /// The repository's advisory list was read in full and compared with OSV's
    /// whole list: what a last page may say it did.
    pub(super) fn compared_in_full(&self) -> bool {
        self.advisories.as_ref().is_some_and(|listing| {
            listing.compared
                && !listing.osv_unanswered
                && advisories::failure(&listing.end).is_none()
        })
    }

    /// OSV did not answer: the repository's advisories are not compared, and
    /// none is named as one OSV does not list.
    pub(super) fn osv_unanswered(&mut self) {
        if let Some(listing) = &mut self.advisories {
            listing.osv_unanswered = true;
        }
    }

    /// Whether GitHub has no repository of the name this version gives.
    pub(super) fn repository_missing(&self) -> bool {
        matches!(
            self.repo,
            Repo::Github {
                read: RepoRead::Missing,
                ..
            }
        )
    }

    /// When GitHub lacks the repository this version names, reads the one
    /// the latest release names, if it is another: a project that moved
    /// lives there, and its advisories are the package's.
    pub(super) async fn follow(&mut self, latest: &Latest, confirmed: &Confirmed, net: &Net<'_>) {
        if !self.repository_missing() {
            return;
        }
        let named = |url: Option<&str>| url.and_then(|url| repository::parse(url).ok());
        let (Some(from), Some(to)) = (
            named(self.registered.repository.as_deref()),
            named(latest.repository.as_deref()),
        ) else {
            return;
        };
        if from.0.eq_ignore_ascii_case(&to.0) && from.1.eq_ignore_ascii_case(&to.1) {
            return;
        }
        let (read, status) = read_github(&to.0, &to.1, net).await;
        if matches!(read, RepoRead::Found(_) | RepoRead::Moved(_)) {
            self.advisories = Some(listing(&to.0, &to.1, confirmed, net).await);
        }
        self.successor = Some(Successor {
            from: format!("{}/{}", from.0, from.1),
            to: format!("{}/{}", to.0, to.1),
            version: latest.version.clone(),
            read,
            status,
        });
    }

    /// The repository read the row states: the latest release's, once GitHub
    /// found it, else the version's own (`read`).
    fn stated_read<'a>(
        &'a self,
        read: &'a RepoRead,
        status: &'static str,
    ) -> (&'a RepoRead, &'static str) {
        match &self.successor {
            Some(successor)
                if matches!(successor.read, RepoRead::Found(_) | RepoRead::Moved(_)) =>
            {
                (&successor.read, successor.status)
            }
            _ => (read, status),
        }
    }

    /// The last page of a resumed walk compares the advisories of the
    /// repository the latest release names, which no earlier page may have
    /// named (its read of the latest release failed, or a release came out
    /// since): the page says which repository that was. Only a comparison
    /// that ran is named: a whole list, set against OSV's answer.
    fn name_compared(&self, data: &mut Value, listing: &Listing) {
        let Some(successor) = &self.successor else {
            return;
        };
        let compared = listing.compared
            && !listing.osv_unanswered
            && advisories::failure(&listing.end).is_none();
        if self.first_page
            || !compared
            || !matches!(successor.read, RepoRead::Found(_) | RepoRead::Moved(_))
        {
            return;
        }
        paging::push_warnings(
            data,
            [format!(
                "This page compared OSV's records with the advisories of {}, which the latest release, {}, names; this version names {}, which GitHub does not show this credential.",
                typed(&successor.to),
                typed(&successor.version),
                typed(&successor.from)
            )],
        );
    }

    /// The repository cautions of `read`, saying which repository moved when
    /// the latest release names another than this version.
    fn repository_cautions(
        &self,
        contract: &SecurityContract,
        read: &RepoRead,
        now: i64,
        today: &str,
    ) -> Vec<Value> {
        let Some(successor) = &self.successor else {
            return repository::cautions(contract, read, now, today);
        };
        let (from, to, version) = (
            typed(&successor.from),
            typed(&successor.to),
            typed(&successor.version),
        );
        match &successor.read {
            RepoRead::Found(_) | RepoRead::Moved(_) => {
                // One move, stated once: a redirect of the latest release's
                // repository is its last step, not a second caution.
                let redirect = match &successor.read {
                    RepoRead::Moved(facts) => format!(
                        ", which GitHub redirects to {}/{}",
                        typed(&facts.owner),
                        typed(&facts.name)
                    ),
                    _ => String::new(),
                };
                let mut found = caution(
                    contract,
                    "repositoryMoved",
                    format!(
                        "This version names {from}, which GitHub does not have; the latest release, {version}, names {to}{redirect}, read {today}. GitHub answers the same for {from} if it is a private repository this credential cannot see."
                    ),
                )
                .into_iter()
                .collect::<Vec<_>>();
                found.extend(
                    repository::cautions(contract, read, now, today)
                        .into_iter()
                        .filter(|caution| caution["rule"] != "repositoryMoved"),
                );
                found
            }
            RepoRead::Missing => caution(
                contract,
                "repositoryMissing",
                format!(
                    "GitHub has neither {from}, which this version names, nor {to}, which the latest release, {version}, names, read {today}. GitHub answers the same for a private repository this credential cannot see."
                ),
            )
            .into_iter()
            .collect(),
            // The latest release's repository was not read: the version's own
            // is still missing, and the row is partial for the unread one.
            RepoRead::Failed(_) => repository::cautions(contract, read, now, today),
        }
    }

    /// Once OSV's walk is whole (`finished`), merges each of the repository's
    /// advisories that shares an id or alias with an OSV record into it, as
    /// GitHub's twin in `results`, and keeps the ids of the rest. Before that,
    /// none is compared: a record OSV lists on a page not read is not "not
    /// listed".
    pub(super) fn merge(&mut self, results: &mut Vec<SourceResult>, finished: bool) {
        let Some(listing) = &mut self.advisories else {
            return;
        };
        if !finished {
            return;
        }
        let known = results
            .iter()
            .flat_map(|result| &result.advisories)
            .collect::<Vec<_>>();
        let twins = advisories::twins(&known, listing.rows.clone());
        listing.others = listing
            .rows
            .iter()
            .map(|row| row.id.clone())
            .filter(|id| !twins.iter().any(|twin| &twin.id == id))
            .collect();
        listing.compared = true;
        if !twins.is_empty() {
            let mut merged = SourceResult::new(Source::GitHub);
            merged.advisories = twins;
            results.push(merged);
        }
    }

    /// Adds `cautions`, `repository` and the GitHub `sources[]` entry.
    pub(super) fn apply(&self, data: &mut Value, brief: &Map<String, Value>) {
        let Ok(contract) = security_contract() else {
            return;
        };
        let now = crate::civil_date::unix_millis();
        let today = crate::civil_date::date_millis(now);
        let whole = self.first_page;
        let mut fired = if whole {
            registry_cautions(contract, &self.registered)
        } else {
            Vec::new()
        };
        let mut source = None;
        match &self.repo {
            Repo::Unnamed => {}
            Repo::Elsewhere(_) if !whole => {}
            Repo::Elsewhere(url) => {
                source = Some(json!({
                    "source": "github", "status": "notCovered", "observedAt": today,
                    "why": format!("The registry names {url}, which is not a GitHub repository."),
                }));
            }
            Repo::Github { read, status } => {
                let (read, status) = self.stated_read(read, status);
                if whole {
                    fired.extend(self.repository_cautions(contract, read, now, &today));
                }
                let mut entry = github_source(read, status, &today);
                if let Some(listing) = &self.advisories {
                    listed(data, &mut entry, listing, read, brief);
                    self.name_compared(data, listing);
                }
                source = Some(entry);
                if whole && let RepoRead::Found(facts) | RepoRead::Moved(facts) = read {
                    let mut block = repository_block(facts, &today);
                    block["provenance"] = json!("claimed");
                    // The stated read is the successor's own: the latest
                    // release named this repository, not the version.
                    if self
                        .successor
                        .as_ref()
                        .is_some_and(|successor| std::ptr::eq(read, &successor.read))
                    {
                        block["namedBy"] = json!("latestRelease");
                    }
                    data["repository"] = block;
                }
            }
        }
        if !fired.is_empty() {
            data["cautions"] = Value::Array(fired);
        }
        super::answer::name_review_cautions(data);
        if let Some(source) = source {
            // The merge of twins listed GitHub as a source of advisories: this
            // entry replaces it, as the one GitHub entry of the row.
            if let Some(sources) = data["sources"].as_array_mut() {
                sources.retain(|entry| entry["source"] != "github");
            }
            match data["sources"].as_array_mut() {
                Some(sources) => sources.push(source),
                None => data["sources"] = json!([source]),
            }
        }
        if let Repo::Github {
            read: RepoRead::Failed(failure),
            ..
        } = &self.repo
        {
            super::source::source_failed(data, std::slice::from_ref(&failure.why));
        }
        if let Some(Successor {
            to,
            read: RepoRead::Failed(failure),
            ..
        }) = &self.successor
        {
            super::source::source_failed(
                data,
                &[format!(
                    "The latest release names {}, which was not read: {}",
                    typed(to),
                    failure.why
                )],
            );
        }
    }
}

/// What the GitHub entry and the row say of the repository's advisory list.
fn listed(
    data: &mut Value,
    entry: &mut Value,
    listing: &Listing,
    read: &RepoRead,
    brief: &Map<String, Value>,
) {
    let (RepoRead::Found(facts) | RepoRead::Moved(facts)) = read else {
        return;
    };
    let repo = format!("{}/{}", facts.owner, facts.name);
    if listing.compared && !listing.others.is_empty() {
        entry["facts"]["repositoryAdvisories"] = json!(listing.others);
        // Ids only here: the repository row lists each with its summary,
        // severity and fixes.
        let mut row = brief.clone();
        row.insert("operation".into(), json!("repository"));
        row.insert("repo".into(), json!(repo));
        data["calls"]["repositoryAdvisories"] = Continuation::related_input(
            ToolId::SecurityCheck,
            json!({ "queries": [Value::Object(row)] }),
        )
        .build();
    }
    if listing.osv_unanswered && !listing.rows.is_empty() {
        let ids = listing
            .rows
            .iter()
            .map(|row| super::answer::stated_id(&row.id))
            .collect::<Vec<_>>();
        paging::push_warnings(
            data,
            [format!(
                "The advisories of {} about this package ({}) were not compared with OSV, which did not answer: none is named as one OSV does not list. The related repository row reads them.",
                super::answer::typed(&repo),
                ids.join(", ")
            )],
        );
        let mut row = brief.clone();
        row.insert("operation".into(), json!("repository"));
        row.insert("repo".into(), json!(repo));
        data["calls"]["repositoryAdvisories"] = Continuation::related_input(
            ToolId::SecurityCheck,
            json!({ "queries": [Value::Object(row)] }),
        )
        .build();
    } else if !listing.compared && !listing.rows.is_empty() {
        paging::push_warnings(
            data,
            [format!(
                "The advisories of {repo} about this package are compared with OSV's records on the last page of OSV's walk: next.nextPage reads on, and none is named before then."
            )],
        );
    }
    if let Some(stopped) = advisories::failure(&listing.end) {
        let why = advisories::unread_why(listing.rows.len(), &stopped, false);
        super::source::note_failure(entry, stopped.code, &why);
        // The entry joins the row's sources after this; it states why.
        super::source::source_failed(data, &[]);
    }
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
