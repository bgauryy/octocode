//! `POST /v1/querybatch`: many OSV queries in one request.
//!
//! OSV answers each query with record ids and `modified` stamps only, in the
//! order asked (`results[i]` answers `queries[i]`), and with `next_page_token`
//! when a query's records exceed its page. The records themselves come from
//! [`super::hydrate`].
use super::super::Net;
use super::{Ask, malformed, post};
use crate::providers::artifact::ArtifactError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// An id list is a search answer: OSV sends no validator or lifetime with
// `/v1/querybatch` (measured 2026-11-10), so a kept list answers a repeat of
// the same request within the disclosed search window
// ([`octocode_github::SEARCH_REUSE_WINDOW`], the window GitHub's search
// answers get) and the row states when it was observed (`observedAt`): a
// reused list is never presented as current. The records it lists are kept
// exactly, under their own `(id, modified)` ([`super::hydrate`]).

/// The most queries one `/v1/querybatch` request holds: OSV's own limit. It
/// answers 1000 and refuses 1001 with HTTP 400 `too many queries` (measured
/// 2026-11-07), so a larger batch is split here and never sent whole.
pub(super) const OSV_QUERYBATCH_MAX_QUERIES: usize = 1000;

/// A record OSV lists for a query: the key of its hydration and of the answer
/// kept for it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(super) struct Ref {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

/// One page of one query to ask: the query and the token that reaches its page.
pub(super) struct PageQuery<'a> {
    pub ask: &'a Ask<'a>,
    pub token: Option<&'a str>,
}

/// What OSV lists on one page of one query.
pub(super) struct Page {
    pub refs: Vec<Ref>,
    pub next: Option<String>,
}

/// The page each query asked, in the order asked. Queries are sent in requests
/// of at most [`OSV_QUERYBATCH_MAX_QUERIES`]; a request that fails fails exactly
/// its own queries, so the pages of the others are kept.
pub(super) async fn pages(
    net: &Net<'_>,
    queries: &[PageQuery<'_>],
) -> Vec<Result<Page, ArtifactError>> {
    let mut answers = Vec::with_capacity(queries.len());
    for chunk in queries.chunks(OSV_QUERYBATCH_MAX_QUERIES) {
        answers.extend(request(net, chunk).await);
    }
    answers
}

/// What an error message calls a request's queries.
fn target(chunk: &[PageQuery<'_>]) -> String {
    match chunk {
        [only] => only.ask.target(),
        [first, rest @ ..] => format!("{} and {} more", first.ask.target(), rest.len()),
        [] => String::new(),
    }
}

async fn request(net: &Net<'_>, chunk: &[PageQuery<'_>]) -> Vec<Result<Page, ArtifactError>> {
    let target = target(chunk);
    let queries = chunk
        .iter()
        .map(|query| query.ask.body(query.token))
        .collect::<Vec<_>>();
    let asked = json!({"queries": queries});
    let failed = |error: ArtifactError| chunk.iter().map(|_| Err(error.clone())).collect();
    let body = match kept_list(net, &asked) {
        Some(body) => body,
        None => match post(net, "querybatch", &asked, &target).await {
            Ok(body) => {
                keep_list(net, &asked, &body);
                body
            }
            Err(error) => return failed(error),
        },
    };
    match results(&body, chunk.len()) {
        Ok(results) => results
            .into_iter()
            .zip(chunk)
            .map(|(result, query)| {
                page(result).map_err(|reason| malformed(&reason, &query.ask.target()))
            })
            .collect::<Vec<_>>(),
        Err(reason) => failed(malformed(&reason, &target)),
    }
}

/// Where a list answer is kept: OSV's origin, no credential (none is sent),
/// and the exact request body.
pub(super) fn list_key(net: &Net<'_>, asked: &Value) -> Option<(String, String)> {
    let origin = url::Url::parse(net.osv_api)
        .ok()?
        .origin()
        .ascii_serialization();
    let scope = octocode_github::Partition::new(origin, None::<&secrecy::SecretString>)
        .identity()
        .to_owned();
    let digest = octocode_engine::digest::sha256(asked.to_string().as_bytes());
    Some((scope, format!("OSV querybatch {digest}")))
}

/// The list answer kept for this exact request, within the search window; its
/// reuse is recorded, so the row discloses when it was observed.
fn kept_list(net: &Net<'_>, asked: &Value) -> Option<Vec<u8>> {
    let (scope, identity) = list_key(net, asked)?;
    let saved = net.answers?.load(&scope, &identity)?;
    let reuse = saved.reuse(
        false,
        Some(octocode_github::SEARCH_REUSE_WINDOW),
        now_millis(),
    );
    if reuse != octocode_github::Reuse::Window {
        return None;
    }
    net.reused.record(saved.observed_ms);
    Some(saved.answer.body.as_bytes().to_vec())
}

/// Keeps a list answer for a repeat of the same request ([`kept_list`]).
fn keep_list(net: &Net<'_>, asked: &Value, body: &[u8]) {
    let (Some(store), Some((scope, identity))) = (net.answers, list_key(net, asked)) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(body) else {
        return;
    };
    store.save(
        &scope,
        &identity,
        octocode_github::SavedAnswer {
            answer: octocode_github::ProviderAnswer {
                request: identity.clone(),
                status: 200,
                headers: Vec::new(),
                body: text.to_owned().into(),
                next: None,
                not_found: None,
            },
            observed_ms: now_millis(),
            redirected: false,
        },
    );
}

fn now_millis() -> u64 {
    u64::try_from(crate::civil_date::unix_millis()).unwrap_or(0)
}

/// The `results` of an answer, one for each of the `asked` queries.
fn results(body: &[u8], asked: usize) -> Result<Vec<Value>, String> {
    let value =
        serde_json::from_slice::<Value>(body).map_err(|_| "a body that is not JSON".to_owned())?;
    let Some(Value::Array(results)) = value.get("results").cloned() else {
        return Err("no results list".to_owned());
    };
    if results.len() != asked {
        return Err(format!("{} results for {asked} queries", results.len()));
    }
    Ok(results)
}

/// One result. `{}` is a finished, empty page; a page with no `vulns` and a
/// token is not empty, only unfinished.
fn page(result: Value) -> Result<Page, String> {
    let Value::Object(mut result) = result else {
        return Err("a result that is not an object".to_owned());
    };
    let refs = match result.remove("vulns") {
        None => Vec::new(),
        Some(Value::Array(listed)) => listed.iter().map(listed_ref).collect::<Result<_, _>>()?,
        Some(_) => return Err("vulns that is not a list".to_owned()),
    };
    Ok(Page {
        refs,
        next: token_of(result.get("next_page_token"))?,
    })
}

fn listed_ref(listed: &Value) -> Result<Ref, String> {
    let Some(id) = listed.get("id").and_then(Value::as_str) else {
        return Err("a listed record with no id".to_owned());
    };
    Ok(Ref {
        id: id.to_owned(),
        modified: listed
            .get("modified")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

fn token_of(value: Option<&Value>) -> Result<Option<String>, String> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(token)) if token.is_empty() => Ok(None),
        Some(Value::String(token)) => Ok(Some(token.clone())),
        Some(_) => Err("a next_page_token that is not text".to_owned()),
    }
}
