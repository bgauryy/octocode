//! OSV walks advanced together: each round asks every walk's next page in
//! `/v1/querybatch` requests, then reads the records those pages list, each
//! distinct one once. One walk is the same code with one query.
//!
//! A page lost to time or a transient failure, or records not read, stop their
//! walk with a saved state that reaches the rest: the page token and the ids not
//! yet read. Nothing is dropped and a failure never reads as clean. A walk that
//! failed with nothing read in this call fails whole: a retry could only repeat
//! it, and its own query is what asks again.
use super::super::Net;
use super::batch::{self, Page, PageQuery, Ref};
use super::hydrate::{self, Hydrated};
use super::{Ask, can_resume, malformed};
use crate::providers::artifact::ArtifactError;
use octocode_contract_types::ErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

/// What a walk asks: a query, and the saved state of an earlier call's walk of it.
pub(super) struct Start<'a> {
    pub ask: &'a Ask<'a>,
    pub resume: Option<&'a str>,
}

/// A walk that read what it could.
pub(super) struct Walked {
    /// Every record read; empty once stopped, as the saved state holds them.
    pub read: Vec<Value>,
    /// The Go package path the records are narrowed to.
    pub import: Option<String>,
    pub stopped: Option<Stopped>,
}

/// Where a stopped walk reads on (opaque to all but this module) and why it stopped.
pub(super) struct Stopped {
    pub saved: String,
    pub reason: String,
}

/// A saved walk. Only this tool issues one. Seen tokens travel with the walk so
/// a transient failure cannot hide a cycle across calls.
#[derive(Deserialize)]
struct SavedWalk {
    pending: Vec<Ref>,
    /// The page that reads on, once `pending` is read; none: the last page was read.
    token: Option<String>,
    read: Vec<Value>,
    seen: BTreeSet<String>,
    #[serde(default)]
    import: Option<String>,
}

#[derive(Serialize)]
struct SavingWalk<'a> {
    pending: &'a [Ref],
    token: Option<&'a str>,
    read: &'a [Value],
    seen: &'a BTreeSet<String>,
    import: &'a Option<String>,
}

enum Next {
    /// The first page has not been asked.
    Start,
    Token(String),
    /// No page follows.
    End,
}

struct Walk<'a> {
    ask: &'a Ask<'a>,
    target: String,
    read: Vec<Value>,
    /// Listed and not yet read.
    pending: Vec<Ref>,
    next: Next,
    seen: BTreeSet<String>,
    import: Option<String>,
    /// This call asked a page or read a record.
    advanced: bool,
    outcome: Option<Result<Walked, ArtifactError>>,
}

impl<'a> Walk<'a> {
    fn begin(start: &Start<'a>) -> Self {
        let mut walk = Self {
            ask: start.ask,
            target: start.ask.target(),
            read: Vec::new(),
            pending: Vec::new(),
            next: Next::Start,
            seen: BTreeSet::new(),
            import: start.ask.import(),
            advanced: false,
            outcome: None,
        };
        if let Some(saved) = start.resume {
            match serde_json::from_str::<SavedWalk>(saved) {
                Ok(saved) => walk.restore(saved),
                Err(_) => {
                    walk.outcome = Some(Err(ArtifactError::new(
                        ErrorCode::InvalidInput,
                        "Invalid saved OSV walk.",
                    )));
                }
            }
        }
        walk
    }

    fn restore(&mut self, saved: SavedWalk) {
        self.read = saved.read;
        self.pending = saved.pending;
        self.seen = saved.seen;
        self.import = saved.import;
        self.next = match saved.token {
            Some(token) => {
                self.seen.insert(token.clone());
                Next::Token(token)
            }
            None => Next::End,
        };
    }

    fn settled(&self) -> bool {
        self.outcome.is_some()
    }

    /// It needs its next page asked.
    fn wants_page(&self) -> bool {
        !self.settled() && self.pending.is_empty() && !matches!(self.next, Next::End)
    }

    fn token(&self) -> Option<&str> {
        match &self.next {
            Next::Token(token) => Some(token),
            Next::Start | Next::End => None,
        }
    }

    fn fail(&mut self, error: ArtifactError) {
        self.outcome = Some(Err(error));
    }

    /// Stops the walk, leaving `unread` ids and the page that follows to the
    /// saved state.
    fn stop(&mut self, unread: &[Ref], reason: String) {
        let saving = SavingWalk {
            pending: unread,
            token: self.token(),
            read: &self.read,
            seen: &self.seen,
            import: &self.import,
        };
        self.outcome = Some(match serde_json::to_string(&saving) {
            Ok(saved) => Ok(Walked {
                read: Vec::new(),
                import: self.import.clone(),
                stopped: Some(Stopped { saved, reason }),
            }),
            Err(_) => Err(malformed("a walk that cannot be saved", &self.target)),
        });
    }

    /// A page, or why it was not read.
    fn take_page(&mut self, answer: Result<Page, ArtifactError>) {
        match answer {
            Ok(page) => {
                self.advanced = true;
                self.pending = page.refs;
                self.next = match page.next {
                    Some(next) if !self.seen.insert(next.clone()) => {
                        let error = malformed("the same page token twice", &self.target);
                        return self.fail(error);
                    }
                    Some(next) => Next::Token(next),
                    None => Next::End,
                };
            }
            Err(error) if self.advanced && self.token().is_some() && can_resume(&error) => {
                self.stop(&[], error.message);
            }
            Err(error) => self.fail(error),
        }
    }

    /// Reads the pending records from `known`; the ones that could not be
    /// read stop the walk, or fail it when nothing was read in this call.
    fn absorb(
        &mut self,
        known: &Hydrated,
        failures: &std::collections::BTreeMap<hydrate::Key, ArtifactError>,
    ) {
        let mut unread = Vec::new();
        let mut first_error = None;
        for listed in std::mem::take(&mut self.pending) {
            let key = hydrate::key(&listed);
            if let Some(record) = known.get(&key) {
                self.read.push(record.clone());
                self.advanced = true;
                continue;
            }
            let error = failures
                .get(&key)
                .cloned()
                .unwrap_or_else(|| malformed("a record it did not return", &listed.id));
            first_error.get_or_insert(error);
            unread.push(listed);
        }
        match first_error {
            Some(error) if self.advanced && can_resume(&error) => self.stop(&unread, error.message),
            Some(error) => self.fail(error),
            None => {}
        }
    }

    fn finish_if_done(&mut self) {
        if !self.settled() && self.pending.is_empty() && matches!(self.next, Next::End) {
            self.outcome = Some(Ok(Walked {
                read: std::mem::take(&mut self.read),
                import: self.import.clone(),
                stopped: None,
            }));
        }
    }

    fn into_outcome(self) -> Result<Walked, ArtifactError> {
        self.outcome
            .unwrap_or_else(|| Err(malformed("a walk that did not end", &self.target)))
    }
}

/// Advances every walk to its end or to the point it stops, in step. The answers
/// are in the order of `starts`.
pub(super) async fn walk_all(
    net: &Net<'_>,
    starts: &[Start<'_>],
) -> Vec<Result<Walked, ArtifactError>> {
    let mut walks = starts.iter().map(Walk::begin).collect::<Vec<_>>();
    let mut known = Hydrated::new();
    while walks.iter().any(|walk| !walk.settled()) {
        read_pages(net, &mut walks).await;
        read_records(net, &mut walks, &mut known).await;
        walks.iter_mut().for_each(Walk::finish_if_done);
    }
    walks.into_iter().map(Walk::into_outcome).collect()
}

/// One walk of one query, from `resume` when it has the saved state of one.
pub(super) async fn walk_one(
    net: &Net<'_>,
    ask: &Ask<'_>,
    resume: Option<&str>,
) -> Result<Walked, ArtifactError> {
    let target = ask.target();
    walk_all(net, &[Start { ask, resume }])
        .await
        .into_iter()
        .next()
        .unwrap_or_else(|| Err(malformed("no answer", &target)))
}

async fn read_pages(net: &Net<'_>, walks: &mut [Walk<'_>]) {
    let due = (0..walks.len())
        .filter(|&index| walks[index].wants_page())
        .collect::<Vec<_>>();
    if due.is_empty() {
        return;
    }
    let queries = due
        .iter()
        .map(|&index| PageQuery {
            ask: walks[index].ask,
            token: walks[index].token(),
        })
        .collect::<Vec<_>>();
    let answers = batch::pages(net, &queries).await;
    for (index, answer) in due.into_iter().zip(answers) {
        walks[index].take_page(answer);
    }
}

async fn read_records(net: &Net<'_>, walks: &mut [Walk<'_>], known: &mut Hydrated) {
    let listed = walks
        .iter()
        .filter(|walk| !walk.settled())
        .flat_map(|walk| &walk.pending)
        .collect::<Vec<_>>();
    if listed.is_empty() {
        return;
    }
    let failures = hydrate::fetch(net, listed, known).await;
    for walk in walks.iter_mut().filter(|walk| !walk.settled()) {
        walk.absorb(known, &failures);
    }
}
