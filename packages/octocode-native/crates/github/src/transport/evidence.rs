//! What one request observed from GitHub: every answer it was sent and the
//! facts it learned on the way (a followed rename, a repository's canonical
//! name, an owner's listing route). A fresh request only records; a request
//! that resumes an issued continuation is seeded with the evidence that
//! continuation owns and answers a recorded request from it, sending
//! nothing, so the evidence it hands on is what its own cursors own.
use bytes::Bytes;
use reqwest::header::{ACCEPT, HeaderMap, HeaderName, HeaderValue, SET_COOKIE};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};
use url::Url;

use super::{HttpMethod, RequestSpec, ResponsePage};
use crate::{ProviderError, ProviderErrorKind};

/// An answer's text as the bytes it arrived in. A clone shares them, so the page
/// a request read, its saved answer and the evidence a continuation owns hold
/// one allocation, however long each lives. Written as a JSON string.
#[derive(Clone, Default, Eq, PartialEq)]
pub struct AnswerBody(Bytes);

impl AnswerBody {
    /// `None` when `bytes` are not UTF-8.
    pub fn from_utf8(bytes: Bytes) -> Option<Self> {
        std::str::from_utf8(&bytes).is_ok().then_some(Self(bytes))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or_default()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<String> for AnswerBody {
    fn from(text: String) -> Self {
        Self(Bytes::from(text))
    }
}

impl From<&str> for AnswerBody {
    fn from(text: &str) -> Self {
        Self(Bytes::copy_from_slice(text.as_bytes()))
    }
}

impl std::fmt::Debug for AnswerBody {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(formatter)
    }
}

impl PartialEq<str> for AnswerBody {
    fn eq(&self, other: &str) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl PartialEq<&str> for AnswerBody {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

impl PartialEq<String> for AnswerBody {
    fn eq(&self, other: &String) -> bool {
        self == other.as_str()
    }
}

impl Serialize for AnswerBody {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for AnswerBody {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer).map(Self::from)
    }
}

/// One answered request as text: GitHub's REST and GraphQL answers are
/// JSON or a file's raw text; a body that is not UTF-8 is never retained.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProviderAnswer {
    /// The request it answers ([`request_key`]).
    pub request: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: AnswerBody,
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

/// The identity of a request: method, URL, whether it reads the first page only,
/// the media type it asked for, its body
/// (a GraphQL query and its variables), and the file it reads when its URL names
/// none. Only the last can hold a line break: a body is one line of JSON.
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
    let file = spec
        .file
        .as_deref()
        .map(|file| format!("\n{file}"))
        .unwrap_or_default();
    format!("{method} {}\n{accept}\n{body}{file}", spec.url)
}

/// The file a request reads when its URL names none, from its [`request_key`].
#[must_use]
pub fn requested_file(request: &str) -> Option<&str> {
    request.splitn(4, '\n').nth(3)
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
        body: answer.body.0.clone(),
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
        body: AnswerBody::from_utf8(page.body.clone())?,
        next: page.next.as_ref().map(Url::to_string),
        not_found: None,
    })
}

struct Recorded {
    request: Arc<str>,
    answer: Result<ResponsePage, ProviderError>,
}

/// Keep the observed request order, while new requests need no scan of all
/// earlier pages. Keys share their allocation with the ordered entries.
#[derive(Default)]
struct RecordedPages {
    requests: HashSet<Arc<str>>,
    answers: Vec<Recorded>,
}

/// One request's journal; clones of its [`super::RequestContext`] share it.
#[derive(Default)]
pub(crate) struct Journal {
    saved: Option<Arc<ProviderEvidence>>,
    /// Only still-usable saved entries are indexed: forgetting a replayed
    /// response must prevent another replay of the same unreadable body.
    saved_requests: Mutex<HashMap<String, usize>>,
    recorded: Mutex<RecordedPages>,
    notes: Mutex<BTreeMap<String, String>>,
}

impl Journal {
    /// A journal resuming `saved`: its facts hold from the start, and its
    /// answers answer the requests they recorded.
    pub(crate) fn resuming(saved: Arc<ProviderEvidence>) -> Self {
        let mut saved_requests = HashMap::with_capacity(saved.answers.len());
        for (index, answer) in saved.answers.iter().enumerate() {
            saved_requests
                .entry(answer.request.clone())
                .or_insert(index);
        }
        Self {
            notes: Mutex::new(saved.notes.clone()),
            saved: Some(saved),
            saved_requests: Mutex::new(saved_requests),
            recorded: Mutex::default(),
        }
    }

    /// The saved answer recorded for `request` (a [`request_key`]).
    fn saved_answer(&self, request: &str) -> Option<&ProviderAnswer> {
        let saved = self.saved.as_ref()?;
        let index = *self
            .saved_requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(request)?;
        saved.answers.get(index)
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
    /// continuation it issues owns, however the page was obtained. A body
    /// its caller stores itself ([`RequestSpec::kept_by_caller`]) is not
    /// recorded: the caller's copy is its only store.
    pub(crate) fn record_page(&self, spec: &RequestSpec, page: &ResponsePage) {
        if !spec.kept_by_caller {
            self.push(request_key(spec), Ok(page.clone()));
        }
    }

    /// Record GitHub's answer to `spec`: a page, or a definitive not-found
    /// (any other failure may answer differently when asked again). A page
    /// its caller stores itself is left out, as in [`Self::record_page`]; a
    /// not-found holds no body and is recorded.
    pub(crate) fn record(&self, spec: &RequestSpec, answer: &Result<ResponsePage, ProviderError>) {
        let recorded = match answer {
            Ok(_) => !spec.kept_by_caller,
            Err(error) => error.kind == ProviderErrorKind::NotFound,
        };
        if recorded {
            self.push(request_key(spec), answer.clone());
        }
    }

    /// Drop the answer recorded for `spec`: an answer that reported a
    /// failure inside a success status (GraphQL `errors`) may answer
    /// differently when asked again, so a continuation asks again.
    pub(crate) fn forget(&self, spec: &RequestSpec) {
        let request = request_key(spec);
        self.saved_requests
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&request);
        let mut recorded = self.recorded.lock().unwrap_or_else(PoisonError::into_inner);
        if recorded.requests.remove(request.as_str()) {
            recorded
                .answers
                .retain(|entry| entry.request.as_ref() != request);
        }
    }

    /// The last answer to a request stands (a GraphQL request retried after
    /// an answer with errors keeps its retry's answer).
    fn push(&self, request: String, answer: Result<ResponsePage, ProviderError>) {
        let mut recorded = self.recorded.lock().unwrap_or_else(PoisonError::into_inner);
        let request: Arc<str> = request.into();
        if recorded.requests.replace(Arc::clone(&request)).is_some() {
            recorded.answers.retain(|entry| entry.request != request);
        }
        recorded.answers.push(Recorded { request, answer });
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

    pub(crate) fn remove_note(&self, name: &str) {
        self.notes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(name);
    }

    /// What this request observed so far: its text answers (a cookie is
    /// never kept) and its facts.
    pub(crate) fn evidence(&self) -> ProviderEvidence {
        let recorded = self.recorded.lock().unwrap_or_else(PoisonError::into_inner);
        ProviderEvidence {
            answers: recorded
                .answers
                .iter()
                .filter_map(|Recorded { request, answer }| match answer {
                    Ok(page) => answer_of(request, page),
                    Err(error) => Some(ProviderAnswer {
                        request: request.to_string(),
                        status: error.status.unwrap_or(404),
                        headers: Vec::new(),
                        body: AnswerBody::default(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgetting_a_replayed_answer_requires_a_fresh_read() {
        let spec = RequestSpec::get(Url::parse("https://api.github.com/repos/a/b").expect("url"));
        let page = ResponsePage {
            status: 200,
            headers: HeaderMap::new(),
            body: Bytes::from_static(b"{\"id\":1}"),
            next: None,
            redirected: false,
        };
        let saved = ProviderEvidence {
            answers: vec![answer_of(&request_key(&spec), &page).expect("answer")],
            notes: BTreeMap::new(),
        };
        let journal = Journal::resuming(Arc::new(saved));
        assert!(journal.answers(&spec));
        assert!(journal.replay(&spec).expect("replay").is_ok());
        journal.forget(&spec);
        assert!(journal.evidence().is_empty());
        assert!(
            !journal.answers(&spec),
            "forgotten evidence cannot answer again"
        );
        assert!(
            journal.replay(&spec).is_none(),
            "a retry must read the origin"
        );
    }

    #[test]
    fn replacing_and_forgetting_answers_preserve_the_last_request_order() {
        let journal = Journal::default();
        let page = |value: &'static [u8]| ResponsePage {
            status: 200,
            headers: HeaderMap::new(),
            body: Bytes::from_static(value),
            next: None,
            redirected: false,
        };
        let first =
            RequestSpec::get(Url::parse("https://api.github.com/repos/a/first").expect("url"));
        let second =
            RequestSpec::get(Url::parse("https://api.github.com/repos/a/second").expect("url"));
        journal.record_page(&first, &page(b"old"));
        journal.record_page(&second, &page(b"second"));
        journal.record_page(&first, &page(b"new"));
        let evidence = journal.evidence();
        assert_eq!(evidence.answers.len(), 2);
        assert_eq!(evidence.answers[0].request, request_key(&second));
        assert_eq!(evidence.answers[1].request, request_key(&first));
        assert_eq!(evidence.answers[1].body, "new");

        journal.forget(&second);
        journal.forget(&second);
        journal.record_page(&second, &page(b"returned"));
        let evidence = journal.evidence();
        assert_eq!(evidence.answers.len(), 2);
        assert_eq!(evidence.answers[0].request, request_key(&first));
        assert_eq!(evidence.answers[1].request, request_key(&second));
        assert_eq!(evidence.answers[1].body, "returned");
    }

    /// A body written and read as JSON is the JSON string a `String` was, and
    /// every hand-off from a page to an answer and back shares its bytes.
    #[test]
    fn a_body_is_a_shared_json_string() {
        let text = "{\"a\":\"\u{e9}\\n\"}\n";
        let body = AnswerBody::from(text);
        let json = serde_json::to_string(&body).expect("json");
        assert_eq!(json, serde_json::to_string(text).expect("json"));
        assert_eq!(
            serde_json::from_str::<AnswerBody>(&json).expect("body"),
            text
        );
        assert!(AnswerBody::from_utf8(Bytes::from_static(&[0xff])).is_none());

        let page = ResponsePage {
            status: 200,
            headers: HeaderMap::new(),
            body: Bytes::copy_from_slice(text.as_bytes()),
            next: None,
            redirected: false,
        };
        let answer = answer_of("GET x", &page).expect("text");
        assert_eq!(answer.body.as_bytes().as_ptr(), page.body.as_ptr());
        assert_eq!(page_of(&answer).body.as_ptr(), page.body.as_ptr());
    }
}
