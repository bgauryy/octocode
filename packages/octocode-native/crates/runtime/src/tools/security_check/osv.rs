//! OSV (`api.osv.dev`), this tool's source of known vulnerabilities.
//!
//! OSV's server decides what a version matches; this module never orders
//! versions. No answer is kept: OSV states no lifetime and no validator. Every
//! request carries only what its caller names, with no credential.
use super::Net;
use super::record::{Selection, rows};
use super::source::{Source, SourceResult, Target};
use crate::providers::artifact::{ArtifactError, ArtifactHttpRequest, JSON, rejection_detail};
use octocode_contract_types::{ArtifactType, ErrorCode};
use octocode_github::ProviderErrorKind;
use serde_json::{Value, json};
use url::Url;

const API: &str = "https://api.osv.dev/v1";

/// What one `/v1/query` asks about.
enum Ask<'a> {
    Package {
        kind: ArtifactType,
        name: &'a str,
        version: Option<&'a str>,
    },
    Commit(&'a str),
}

impl Ask<'_> {
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
    /// alone (OSV refuses a version beside it), and the token.
    fn body(&self, token: Option<&str>) -> Value {
        let mut body = match self {
            Self::Package {
                kind,
                name,
                version,
            } => {
                let mut body = json!({"package": {"name": name, "ecosystem": ecosystem(*kind)}});
                if let Some(version) = version {
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
                    kind: confirmed.kind,
                    name: &confirmed.name,
                    version: confirmed.version.as_deref(),
                },
                &Selection::Named(confirmed.kind, &names),
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

/// Every `/v1/query` page from `start` until no token follows. A page with no
/// `vulns` and a token is not empty: only the end of the walk says so. A later
/// page lost to time or a transient failure saves what was read with the token
/// that reaches the rest, so the row lists its advisories once the walk finished
/// or not at all; anything else fails whole, so a record is never dropped and a
/// failure never reads as clean.
async fn walk(
    net: &Net<'_>,
    ask: &Ask<'_>,
    selection: &Selection<'_>,
    start: Option<&str>,
) -> Result<SourceResult, ArtifactError> {
    let target = ask.target();
    let (first, mut read) = walk_state(start)?;
    let mut token = first.clone();
    loop {
        let (records, next) = match query_page(net, ask, token.as_deref()).await {
            Ok(page) => page,
            Err(error) => {
                return match token {
                    Some(token) if can_resume(&error) && first.as_ref() != Some(&token) => {
                        let saved = json!({"token": token, "read": read}).to_string();
                        Ok(SourceResult::new(Source::Osv).stopped(saved, error.message))
                    }
                    _ => Err(error),
                };
            }
        };
        read.extend(records);
        match next {
            Some(next) if token.as_deref() == Some(next.as_str()) => {
                return Err(malformed("the same page token twice", &target));
            }
            Some(next) => token = Some(next),
            None => {
                let mut result = SourceResult::new(Source::Osv);
                result.advisories = rows(&read, selection, &target)?;
                return Ok(result);
            }
        }
    }
}

/// A saved walk: OSV's token and the records read before it. Only this tool
/// issues one.
fn walk_state(start: Option<&str>) -> Result<(Option<String>, Vec<Value>), ArtifactError> {
    let Some(start) = start else {
        return Ok((None, Vec::new()));
    };
    let invalid = || ArtifactError::new(ErrorCode::InvalidInput, "Invalid saved OSV walk.");
    let saved: Value = serde_json::from_str(start).map_err(|_| invalid())?;
    let token = saved["token"].as_str().ok_or_else(invalid)?.to_owned();
    let read = saved["read"].as_array().ok_or_else(invalid)?.clone();
    Ok((Some(token), read))
}

/// One record by id, withdrawn ones included. An id OSV does not have is not
/// found, never clean.
async fn check_advisory(net: &Net<'_>, id: &str) -> Result<SourceResult, ArtifactError> {
    let Some(record) = by_id(net, id).await? else {
        return Err(ArtifactError::new(
            ErrorCode::NotFound,
            format!("OSV has no record {id}: this says nothing about whether it exists elsewhere."),
        )
        .with_status(404));
    };
    let mut result = SourceResult::new(Source::Osv);
    result.advisories = rows(&[record], &Selection::All, id)?;
    Ok(result)
}

/// The OSV name of each registry, exactly as OSV spells it: its requests are
/// case-sensitive. No wildcard: a new registry does not compile until someone
/// decides what OSV calls it.
pub(super) fn ecosystem(kind: ArtifactType) -> &'static str {
    match kind {
        ArtifactType::Npm => "npm",
        ArtifactType::Pypi => "PyPI",
        ArtifactType::Crates => "crates.io",
        ArtifactType::Maven => "Maven",
        ArtifactType::Nuget => "NuGet",
        ArtifactType::Go => "Go",
        ArtifactType::Packagist => "Packagist",
        ArtifactType::Rubygems => "RubyGems",
    }
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

fn request(path: &str) -> Result<ArtifactHttpRequest, ArtifactError> {
    let url = Url::parse(&format!("{API}/{path}"))
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
        format!(
            "OSV answered with {reason}: advisories for {target} were not checked. Retry later."
        ),
    )
}

async fn post(
    net: &Net<'_>,
    path: &str,
    body: &Value,
    target: &str,
) -> Result<Vec<u8>, ArtifactError> {
    let response = net
        .http
        .post(request(path)?, body.to_string().into_bytes(), net.budget)
        .await
        .map_err(|error| transport_failure(error, target))?;
    if (200..300).contains(&response.status) {
        return Ok(response.body);
    }
    Err(status_failure(response.status, &response.body, target))
}

/// One `/v1/query` page: its records and next token. `{}` is a finished, empty
/// page; a page with no `vulns` and a token is not empty, only unfinished.
async fn query_page(
    net: &Net<'_>,
    ask: &Ask<'_>,
    token: Option<&str>,
) -> Result<(Vec<Value>, Option<String>), ArtifactError> {
    let target = ask.target();
    let body = post(net, "query", &ask.body(token), &target).await?;
    parsed(&body).map_err(|reason| malformed(&reason, &target))
}

fn parsed(body: &[u8]) -> Result<(Vec<Value>, Option<String>), String> {
    let value =
        serde_json::from_slice::<Value>(body).map_err(|_| "a body that is not JSON".to_owned())?;
    let Value::Object(mut page) = value else {
        return Err("a body that is not an object".to_owned());
    };
    let records = match page.remove("vulns") {
        None => Vec::new(),
        Some(Value::Array(records)) => records,
        Some(_) => return Err("vulns that is not a list".to_owned()),
    };
    Ok((records, token_of(page.get("next_page_token"))?))
}

fn token_of(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(token)) if token.is_empty() => Ok(None),
        Some(Value::String(token)) => Ok(Some(token.clone())),
        Some(_) => Err("a next_page_token that is not text".to_owned()),
    }
}

/// One record by id, withdrawn ones included; `None` when OSV has no such id.
async fn by_id(net: &Net<'_>, id: &str) -> Result<Option<Value>, ArtifactError> {
    let encoded: String = url::form_urlencoded::byte_serialize(id.as_bytes()).collect();
    let response = net
        .http
        .get(request(&format!("vulns/{encoded}"))?, net.budget)
        .await
        .map_err(|error| transport_failure(error, id))?;
    match response.status {
        200..=299 => serde_json::from_slice(&response.body)
            .map(Some)
            .map_err(|_| malformed("a body that is not JSON", id)),
        404 => Ok(None),
        status => Err(status_failure(status, &response.body, id)),
    }
}

#[cfg(test)]
mod tests;
