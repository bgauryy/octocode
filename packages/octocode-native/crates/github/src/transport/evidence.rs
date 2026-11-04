//! What one request observed from GitHub: every answer it was sent and the
//! facts it learned on the way (a followed rename, a repository's canonical
//! name, an owner's listing route). A fresh request only records; a request
//! that resumes an issued continuation is seeded with the evidence that
//! continuation owns and answers a recorded request from it, sending
//! nothing, so the evidence it hands on is what its own cursors own.
use bytes::Bytes;
use reqwest::header::{ACCEPT, HeaderMap, HeaderName, HeaderValue, SET_COOKIE};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use url::Url;

use super::{HttpMethod, RequestSpec, ResponsePage};
use crate::{ProviderError, ProviderErrorKind};

/// One answered request as text: GitHub's REST and GraphQL answers are
/// JSON or a file's raw text; a body that is not UTF-8 is never retained.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderAnswer {
    /// The request it answers ([`request_key`]).
    pub request: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub next: Option<String>,
    /// GitHub's definitive answer that the resource does not exist (a
    /// route probe, a missing path at a pinned commit).
    pub not_found: Option<ProviderError>,
}

/// The answers and facts one request observed, in request order.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderEvidence {
    pub answers: Vec<ProviderAnswer>,
    /// Request-scoped facts by name ([`Journal::note`]).
    pub notes: BTreeMap<String, String>,
}

impl ProviderEvidence {
    pub fn is_empty(&self) -> bool {
        self.answers.is_empty() && self.notes.is_empty()
    }

    /// About how many bytes the evidence holds.
    pub fn bytes(&self) -> usize {
        let answers = self.answers.iter().map(|answer| {
            answer.request.len()
                + answer.body.len()
                + answer.next.as_ref().map_or(0, String::len)
                + answer
                    .headers
                    .iter()
                    .map(|(name, value)| name.len() + value.len())
                    .sum::<usize>()
        });
        let notes = self
            .notes
            .iter()
            .map(|(name, value)| name.len() + value.len());
        answers.chain(notes).sum()
    }
}

/// The identity of a request: method, URL, the media type it asked for and
/// its body (a GraphQL query and its variables).
pub(super) fn request_key(spec: &RequestSpec) -> String {
    let method = match spec.method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
    };
    let accept = spec
        .headers
        .get(ACCEPT)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let body = spec
        .body
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    format!("{method} {}\n{accept}\n{body}", spec.url)
}

/// A saved answer as the page it answers with.
pub(super) fn page_of(answer: &ProviderAnswer) -> ResponsePage {
    ResponsePage {
        status: answer.status,
        headers: answer
            .headers
            .iter()
            .filter_map(|(name, value)| {
                Some((
                    HeaderName::from_bytes(name.as_bytes()).ok()?,
                    HeaderValue::from_str(value).ok()?,
                ))
            })
            .collect::<HeaderMap>(),
        body: Bytes::from(answer.body.clone()),
        next: answer
            .next
            .as_deref()
            .and_then(|next| Url::parse(next).ok()),
        redirected: false,
    }
}

/// `page` as the text answer to `request`; `None` when it is not text (a
/// body that is not UTF-8 is never retained). A cookie is never kept.
pub(super) fn answer_of(request: &str, page: &ResponsePage) -> Option<ProviderAnswer> {
    Some(ProviderAnswer {
        request: request.to_owned(),
        status: page.status,
        headers: page
            .headers
            .iter()
            .filter(|(name, _)| *name != SET_COOKIE)
            .map(|(name, value)| Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned())))
            .collect::<Option<Vec<_>>>()?,
        body: String::from_utf8(page.body.to_vec()).ok()?,
        next: page.next.as_ref().map(Url::to_string),
        not_found: None,
    })
}

struct Recorded {
    request: String,
    answer: Result<ResponsePage, ProviderError>,
}

/// One request's journal; clones of its [`super::RequestContext`] share it.
#[derive(Default)]
pub(crate) struct Journal {
    saved: Option<Arc<ProviderEvidence>>,
    recorded: Mutex<Vec<Recorded>>,
    notes: Mutex<BTreeMap<String, String>>,
}

impl Journal {
    /// A journal resuming `saved`: its facts hold from the start, and its
    /// answers answer the requests they recorded.
    pub(crate) fn resuming(saved: Arc<ProviderEvidence>) -> Self {
        Self {
            notes: Mutex::new(saved.notes.clone()),
            saved: Some(saved),
            recorded: Mutex::default(),
        }
    }

    /// The saved answer recorded for `request` (a [`request_key`]).
    fn saved_answer(&self, request: &str) -> Option<&ProviderAnswer> {
        self.saved
            .as_ref()?
            .answers
            .iter()
            .find(|answer| answer.request == request)
    }

    /// Whether a saved answer answers `spec`: [`Self::replay`] sends nothing
    /// to the provider for it.
    pub(crate) fn answers(&self, spec: &RequestSpec) -> bool {
        self.saved_answer(&request_key(spec)).is_some()
    }

    /// The saved answer to `spec`, recorded again as this request's own.
    pub(crate) fn replay(&self, spec: &RequestSpec) -> Option<Result<ResponsePage, ProviderError>> {
        let request = request_key(spec);
        let answer = self.saved_answer(&request)?;
        if let Some(error) = &answer.not_found {
            self.push(request, Err(error.clone()));
            return Some(Err(error.clone()));
        }
        let page = page_of(answer);
        self.push(request, Ok(page.clone()));
        Some(Ok(page))
    }

    /// Record `page` as the answer this request saw for `spec`: what a
    /// continuation it issues owns, however the page was obtained.
    pub(crate) fn record_page(&self, spec: &RequestSpec, page: &ResponsePage) {
        self.push(request_key(spec), Ok(page.clone()));
    }

    /// Record GitHub's answer to `spec`: a page, or a definitive not-found
    /// (any other failure may answer differently when asked again).
    pub(crate) fn record(&self, spec: &RequestSpec, answer: &Result<ResponsePage, ProviderError>) {
        if answer
            .as_ref()
            .err()
            .is_none_or(|error| error.kind == ProviderErrorKind::NotFound)
        {
            self.push(request_key(spec), answer.clone());
        }
    }

    /// Drop the answer recorded for `spec`: an answer that reported a
    /// failure inside a success status (GraphQL `errors`) may answer
    /// differently when asked again, so a continuation asks again.
    pub(crate) fn forget(&self, spec: &RequestSpec) {
        let request = request_key(spec);
        self.recorded
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|entry| entry.request != request);
    }

    /// The last answer to a request stands (a GraphQL request retried after
    /// an answer with errors keeps its retry's answer).
    fn push(&self, request: String, answer: Result<ResponsePage, ProviderError>) {
        let mut recorded = self.recorded.lock().unwrap_or_else(PoisonError::into_inner);
        recorded.retain(|entry| entry.request != request);
        recorded.push(Recorded { request, answer });
    }

    pub(crate) fn note(&self, name: &str) -> Option<String> {
        self.notes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(name)
            .cloned()
    }

    pub(crate) fn set_note(&self, name: String, value: String) {
        self.notes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(name, value);
    }

    /// What this request observed so far: its text answers (a cookie is
    /// never kept) and its facts.
    pub(crate) fn evidence(&self) -> ProviderEvidence {
        let recorded = self.recorded.lock().unwrap_or_else(PoisonError::into_inner);
        ProviderEvidence {
            answers: recorded
                .iter()
                .filter_map(|Recorded { request, answer }| match answer {
                    Ok(page) => answer_of(request, page),
                    Err(error) => Some(ProviderAnswer {
                        request: request.clone(),
                        status: error.status.unwrap_or(404),
                        headers: Vec::new(),
                        body: String::new(),
                        next: None,
                        not_found: Some(error.clone()),
                    }),
                })
                .collect(),
            notes: self
                .notes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        }
    }
}
