//! Trust signals of a package row: what the registry says of the version
//! (deprecated, yanked, its source repository) and what GitHub says of that
//! repository. Facts and cautions, never a verdict; a GitHub that did not answer
//! is disclosed with its reason and leaves the OSV answer standing.
use super::Net;
use super::advisories::{self, End};
use super::record::{SecurityAdvisory, Selection};
use super::repository::{
    self, RepoRead, caution, github_source, policy_block, read_github, repository_block,
};
use super::source::{Confirmed, Source, SourceResult};
use crate::contracts::security::{SecurityContract, security_contract};
use crate::providers::artifact::ArtifactItem;
use crate::response::paging::{self, PartialReason};
use serde_json::{Value, json};

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
    /// The source repository's advisories about this package, once GitHub found it.
    advisories: Option<Listing>,
}

/// The source repository's advisories that name the package: a monorepo's
/// advisory about another of its packages only is not this package's.
struct Listing {
    rows: Vec<SecurityAdvisory>,
    end: End,
    /// Compared with OSV's records: false until OSV's walk is whole.
    compared: bool,
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
        advisories,
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

/// The answer names each caution of level `review` by its rule, so a sentence
/// that says "none known" does not stand alone beside cautions to look at. Rule
/// names come from the contract; the raw text stays in `cautions[].why`.
fn name_review_cautions(data: &mut Value, fired: &[Value]) {
    let rules = fired
        .iter()
        .filter(|caution| caution["level"] == "review")
        .filter_map(|caution| caution["rule"].as_str())
        .collect::<Vec<_>>();
    let Some(answer) = data["answer"].as_str() else {
        return;
    };
    if !rules.is_empty() {
        data["answer"] = json!(format!(
            "{answer} Review before relying on it: {}; see cautions.",
            rules.join(", ")
        ));
    }
}

impl Trust {
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

    /// Adds `cautions`, `repository`, `policy` and the GitHub `sources[]` entry.
    pub(super) fn apply(&self, data: &mut Value) {
        let Ok(contract) = security_contract() else {
            return;
        };
        let now = crate::civil_date::unix_millis();
        let today = crate::civil_date::date_millis(now);
        let mut fired = registry_cautions(contract, &self.registered);
        let mut source = None;
        match &self.repo {
            Repo::Unnamed => {}
            Repo::Elsewhere(url) => {
                source = Some(json!({
                    "source": "github", "status": "notCovered", "observedAt": today,
                    "why": format!("The registry names {url}, which is not a GitHub repository."),
                }));
            }
            Repo::Github { read, status } => {
                fired.extend(repository::cautions(contract, read, now, &today));
                let mut entry = github_source(read, status, &today);
                if let Some(listing) = &self.advisories {
                    listed(data, &mut entry, listing, read);
                }
                source = Some(entry);
                if let RepoRead::Found(facts) | RepoRead::Moved(facts) = read {
                    let mut block = repository_block(facts, &today);
                    block["provenance"] = json!("claimed");
                    data["repository"] = block;
                }
                if !matches!(read, RepoRead::Failed(_)) {
                    data["policy"] = policy_block(contract);
                }
            }
        }
        name_review_cautions(data, &fired);
        if !fired.is_empty() {
            data["cautions"] = Value::Array(fired);
        }
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
            paging::mark_partial(
                data,
                PartialReason::SourceFailed,
                std::slice::from_ref(&failure.why),
            );
        }
    }
}

/// What the GitHub entry and the row say of the repository's advisory list.
fn listed(data: &mut Value, entry: &mut Value, listing: &Listing, read: &RepoRead) {
    let (RepoRead::Found(facts) | RepoRead::Moved(facts)) = read else {
        return;
    };
    let repo = format!("{}/{}", facts.owner, facts.name);
    if listing.compared && !listing.others.is_empty() {
        entry["facts"]["repositoryAdvisories"] = json!(listing.others);
    }
    if !listing.compared && !listing.rows.is_empty() {
        paging::push_warnings(
            data,
            [format!(
                "The advisories of {repo} about this package were not compared with OSV's records: OSV's walk is not whole on this page."
            )],
        );
    }
    if let Some(stopped) = advisories::failure(&listing.end) {
        let why = advisories::unread_why(listing.rows.len(), &stopped, false);
        entry["errorCode"] = json!(stopped.code);
        entry["why"] = json!(why);
        paging::mark_partial(
            data,
            PartialReason::SourceFailed,
            std::slice::from_ref(&why),
        );
    }
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
