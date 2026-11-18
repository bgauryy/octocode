//! OSV (`api.osv.dev`), this tool's source of known vulnerabilities.
//!
//! OSV's server decides what a version matches; this module never orders
//! versions. Package and commit queries go through `/v1/querybatch` (ids only,
//! [`batch`]) and each listed record is read once by `/v1/vulns/{id}`
//! ([`hydrate`]); only a record under its own `(id, modified)` is kept, since
//! OSV states no lifetime for anything else. Every request carries only what its
//! caller names, with no credential.
use super::Net;
use super::ecosystem::{self, Ecosystem, osv_name};
use super::record::{Selection, rows};
use super::source::{Confirmed, Source, SourceResult, Target};
use crate::artifact::{
    ArtifactError, ArtifactHttpRequest, ArtifactHttpResponse, JSON, rejection_detail,
};
use octocode_contract_types::ErrorCode;
use octocode_github::ProviderErrorKind;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use url::Url;

/// Records read at once: how politely one call asks the public API, not a limit
/// on how many it reads.
pub(super) const CONCURRENT_READS: usize = 8;

/// What one OSV query asks about.
enum Ask<'a> {
    Package {
        ecosystem: Ecosystem,
        release: Option<&'a str>,
        name: &'a str,
        version: Option<&'a str>,
        /// The Go package path inside `name`, when `name` is its module.
        import: Option<&'a str>,
        /// OSV cannot order this ecosystem's versions: it is asked by name and
        /// `version` is compared here against each record's ranges.
        by_name: bool,
    },
    Commit(&'a str),
}

impl Ask<'_> {
    /// The Go package path the records are narrowed to.
    fn import(&self) -> Option<String> {
        match self {
            Self::Package { import, .. } => import.map(str::to_owned),
            Self::Commit(_) => None,
        }
    }

    /// The version compared here, when OSV cannot order the ecosystem's.
    fn compared(&self) -> Option<&str> {
        match self {
            Self::Package {
                by_name: true,
                version,
                ..
            } => *version,
            _ => None,
        }
    }

    /// As a message states it.
    fn target(&self) -> String {
        match self {
            Self::Package {
                name,
                version: Some(version),
                ..
            } => format!("{name}@{version}"),
            Self::Package { name, .. } => (*name).to_owned(),
            Self::Commit(commit) => format!("commit {commit}"),
        }
    }

    /// The whole query: nothing but the package and version, or the commit
    /// alone (OSV refuses a version beside it), and the page token.
    fn body(&self, token: Option<&str>) -> Value {
        let mut body = match self {
            Self::Package {
                ecosystem,
                release,
                name,
                version,
                by_name,
                ..
            } => {
                let mut body =
                    json!({"package": {"name": name, "ecosystem": osv_name(*ecosystem, *release)}});
                if let Some(version) = version.filter(|_| !by_name) {
                    body["version"] = json!(version);
                }
                body
            }
            Self::Commit(commit) => json!({"commit": commit}),
        };
        if let Some(token) = token {
            body["page_token"] = json!(token);
        }
        body
    }
}

/// What OSV knows about a target.
pub(super) async fn ask(target: &Target, net: &Net<'_>) -> Result<SourceResult, ArtifactError> {
    match target {
        Target::Package(confirmed) => {
            let names = [confirmed.name.clone()];
            walk(
                net,
                &Ask::Package {
                    ecosystem: confirmed.ecosystem,
                    release: confirmed.release.as_deref(),
                    name: &confirmed.name,
                    version: confirmed.version.as_deref(),
                    import: confirmed.import.as_deref(),
                    by_name: !ecosystem::osv_orders_versions(confirmed.ecosystem)?,
                },
                &Selection::Named(confirmed.ecosystem, &names, confirmed.release.as_deref()),
                confirmed.resume.as_deref(),
            )
            .await
        }
        Target::Commit { commit, resume } => {
            walk(
                net,
                &Ask::Commit(commit),
                &Selection::All,
                resume.as_deref(),
            )
            .await
        }
        Target::Advisory(id) => check_advisory(net, id).await,
    }
}

/// Every page of the query from `start` until no token follows, and every record
/// it lists. A page with no `vulns` and a token is not empty: only the end of
/// the walk says so. A later page or record lost to time or a transient failure
/// saves what was read with the state that reaches the rest, so the row lists
/// its advisories once the walk finished or not at all; anything else fails
/// whole, so a record is never dropped and a failure never reads as clean.
async fn walk(
    net: &Net<'_>,
    ask: &Ask<'_>,
    selection: &Selection<'_>,
    start: Option<&str>,
) -> Result<SourceResult, ArtifactError> {
    let target = ask.target();
    let walked = walk::walk_one(net, ask, start).await?;
    if let Some(stopped) = walked.stopped {
        return Ok(SourceResult::new(Source::Osv).stopped(stopped.saved, stopped.reason));
    }
    let mut result = SourceResult::new(Source::Osv);
    let (kept, apart) = imports::split(walked.read, walked.import.as_deref(), selection);
    let kept = match ask.compared() {
        Some(version) => {
            let narrowed = versions::narrow(kept, selection, version);
            result.compared_here = true;
            result.unmatched = narrowed.unmatched;
            narrowed.kept
        }
        None => kept,
    };
    result.advisories = rows(&kept, selection, &target)?;
    result.module_only = rows(&apart, selection, &target)?;
    Ok(result)
}

/// Whether OSV lists the package in the release at all, whatever the version.
/// OSV answers a version of a release it does not name, and one no advisory
/// covers, with the same nothing; any record of the package in that release
/// says it names the release. A package with no version asked is already asked by name.
pub(super) async fn names_release(
    confirmed: &Confirmed,
    net: &Net<'_>,
) -> Result<bool, ArtifactError> {
    let ask = Ask::Package {
        ecosystem: confirmed.ecosystem,
        release: confirmed.release.as_deref(),
        name: &confirmed.name,
        version: None,
        import: None,
        by_name: false,
    };
    let target = ask.target();
    let mut seen = BTreeSet::new();
    let mut token: Option<String> = None;
    loop {
        let query = batch::PageQuery {
            ask: &ask,
            token: token.as_deref(),
        };
        let page = batch::pages(net, &[query])
            .await
            .pop()
            .unwrap_or_else(|| Err(malformed("no answer", &target)))?;
        if !page.refs.is_empty() {
            return Ok(true);
        }
        let Some(next) = page.next else {
            return Ok(false);
        };
        if !seen.insert(next.clone()) {
            return Err(malformed("the same page token twice", &target));
        }
        token = Some(next);
    }
}

/// The id of every record OSV keeps for the package in the release, whatever
/// the version: the pages of one query by name. What tells whether OSV keeps a
/// record of the package, for each of many ids, without reading each one.
pub(super) async fn record_ids(
    confirmed: &Confirmed,
    net: &Net<'_>,
) -> Result<BTreeSet<String>, ArtifactError> {
    let ask = Ask::Package {
        ecosystem: confirmed.ecosystem,
        release: confirmed.release.as_deref(),
        name: &confirmed.name,
        version: None,
        import: None,
        by_name: false,
    };
    let target = ask.target();
    let mut ids = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut token: Option<String> = None;
    loop {
        let query = batch::PageQuery {
            ask: &ask,
            token: token.as_deref(),
        };
        let page = batch::pages(net, &[query])
            .await
            .pop()
            .unwrap_or_else(|| Err(malformed("no answer", &target)))?;
        ids.extend(page.refs.into_iter().map(|listed| listed.id));
        let Some(next) = page.next else {
            return Ok(ids);
        };
        if !seen.insert(next.clone()) {
            return Err(malformed("the same page token twice", &target));
        }
        token = Some(next);
    }
}

/// One record by id, withdrawn ones included. An id OSV does not have is not
/// found, never clean.
async fn check_advisory(net: &Net<'_>, id: &str) -> Result<SourceResult, ArtifactError> {
    let Some(fetched) = by_id(net, id).await? else {
        return Err(ArtifactError::new(
            ErrorCode::NotFound,
            format!("OSV has no record {id}: this says nothing about whether it exists elsewhere."),
        )
        .with_status(404));
    };
    let mut result = SourceResult::new(Source::Osv);
    result.advisories = rows(&[fetched.record], &Selection::All, id)?;
    Ok(result)
}

/// Whether the rest of a walk stays reachable after `error`: time ran out, or
/// OSV failed in a way a later call may not. A rejected request or a cancelled
/// call does not resume.
fn can_resume(error: &ArtifactError) -> bool {
    match (error.code, error.status) {
        (ErrorCode::Cancelled, _) => false,
        (_, Some(status)) => octocode_io::retry::classify_status(status).is_retryable(),
        (_, None) => true,
    }
}

fn request(net: &Net<'_>, path: &str) -> Result<ArtifactHttpRequest, ArtifactError> {
    let url = Url::parse(&format!("{}/{path}", net.osv_api))
        .map_err(|_| ArtifactError::new(ErrorCode::InvalidInput, "Invalid OSV request."))?;
    Ok(ArtifactHttpRequest {
        url,
        accept: JSON,
        authorization: None,
        dns_pin: None,
        validator: None,
    })
}

/// The shared transport names its usual peer, a registry.
fn transport_failure(mut error: ArtifactError, target: &str) -> ArtifactError {
    let what = error.message.replacen("Artifact registry", "OSV", 1);
    error.message = format!(
        "{}: advisories for {target} were not checked.",
        what.trim_end_matches('.')
    );
    error
}

/// Why OSV did not answer, from the failure's code and HTTP status only: an
/// error's message can hold the id asked and OSV's own text, and a sentence
/// built from source data states neither.
pub(super) fn unread_reason(error: &ArtifactError) -> String {
    let code = serde_json::to_value(error.code)
        .ok()
        .and_then(|code| code.as_str().map(str::to_owned))
        .unwrap_or_default();
    match error.status {
        Some(status) => format!("OSV did not answer: HTTP {status}, {code}"),
        None if answered_unreadably(error) => {
            format!("OSV answered, but not with records this tool could read: {code}")
        }
        None => format!("OSV did not answer: {code}"),
    }
}

/// How [`malformed`] opens its message: OSV answered, with what this tool
/// could not read.
const ANSWERED_WITH: &str = "OSV answered with ";

/// OSV answered, but not with records this tool could read ([`malformed`]),
/// unlike an answer that never came.
pub(super) fn answered_unreadably(error: &ArtifactError) -> bool {
    error.status.is_none() && error.message.starts_with(ANSWERED_WITH)
}

/// A non-2xx answer under the status table GitHub answers share; the message
/// names OSV, not a registry.
fn status_failure(status: u16, body: &[u8], target: &str) -> ArtifactError {
    let mut what = format!("OSV did not answer (HTTP {status})");
    if let Some(detail) = rejection_detail(body) {
        what = format!("{what}, saying \"{detail}\"");
    }
    let mut message = format!("{what}: advisories for {target} were not checked.");
    if octocode_io::retry::classify_status(status).is_retryable() {
        message.push_str(" Retry later.");
    }
    ArtifactError::new(ProviderErrorKind::from_status(status).code(), message).with_status(status)
}

pub(super) fn malformed(reason: &str, target: &str) -> ArtifactError {
    ArtifactError::new(
        ErrorCode::ProviderError,
        format!("{ANSWERED_WITH}{reason}: advisories for {target} were not checked. Retry later."),
    )
}

async fn post(
    net: &Net<'_>,
    path: &str,
    body: &Value,
    target: &str,
) -> Result<ArtifactHttpResponse, ArtifactError> {
    let response = net
        .http
        .post(
            request(net, path)?,
            body.to_string().into_bytes(),
            net.budget,
        )
        .await
        .map_err(|error| transport_failure(error, target))?;
    if (200..300).contains(&response.status) {
        return Ok(response);
    }
    Err(status_failure(response.status, &response.body, target))
}

/// A decoded record with the origin policy that accompanied that exact body.
/// Internal callers return the record value; hydration also needs its policy.
pub(super) struct FetchedRecord {
    pub(super) record: Value,
    pub(super) status: u16,
    pub(super) headers: Vec<(String, String)>,
}

/// One record by id, withdrawn ones included; `None` when OSV has no such id.
pub(super) async fn by_id(net: &Net<'_>, id: &str) -> Result<Option<FetchedRecord>, ArtifactError> {
    let encoded: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
    let response = net
        .http
        .get(request(net, &format!("vulns/{encoded}"))?, net.budget)
        .await
        .map_err(|error| transport_failure(error, id))?;
    match response.status {
        200..=299 => {
            let record = serde_json::from_slice(&response.body)
                .map_err(|_| malformed("a body that is not JSON", id))?;
            Ok(Some(FetchedRecord {
                record,
                status: response.status,
                headers: response.headers,
            }))
        }
        404 => Ok(None),
        status => Err(status_failure(status, &response.body, id)),
    }
}

mod batch;
mod hydrate;
mod imports;
mod versions;
mod walk;

pub(super) use versions::written_ranges_clear;

#[cfg(test)]
mod tests;
