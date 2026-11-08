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
    let answered = post(net, "querybatch", &json!({"queries": queries}), &target).await;
    let failed = |error: ArtifactError| chunk.iter().map(|_| Err(error.clone())).collect();
    let body = match answered {
        Ok(body) => body,
        Err(error) => return failed(error),
    };
    match results(&body, chunk.len()) {
        Ok(results) => results
            .into_iter()
            .zip(chunk)
            .map(|(result, query)| {
                page(result).map_err(|reason| malformed(&reason, &query.ask.target()))
            })
            .collect(),
        Err(reason) => failed(malformed(&reason, &target)),
    }
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
