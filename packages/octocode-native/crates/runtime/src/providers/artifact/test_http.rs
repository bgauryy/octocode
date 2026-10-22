//! Registry and GitHub-tag doubles shared by the artifact tests.
use super::http::{
    ArtifactHttp, ArtifactHttpFuture, ArtifactHttpRequest, ArtifactHttpResponse, BodySink,
    StatusFuture,
};
use super::release_ref::{ReleaseTags, TagFuture};
use octocode_io::budget::RequestBudget;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// One request as a double received it.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    pub url: url::Url,
    pub accept: &'static str,
    pub authorized: bool,
}

impl Seen {
    /// The request's path, with its query when it has one.
    pub(crate) fn target(&self) -> String {
        match self.url.query() {
            Some(query) => format!("{}?{query}", self.url.path()),
            None => self.url.path().to_owned(),
        }
    }
}

struct Route {
    /// A request's `path?query`, else its path, equals this.
    target: String,
    /// Answered only for this `Accept`, when set.
    accept: Option<&'static str>,
    status: u16,
    body: Vec<u8>,
}

/// Answers by route: a request whose `path?query`, else whose path, names a
/// route gets that route's status (200 unless it names one) and body (a later
/// route for the same target overrides an earlier one); any other request
/// gets the `otherwise` body, else a 404. Records every request.
#[derive(Default)]
pub(crate) struct RouteHttp {
    routes: Vec<Route>,
    otherwise: Option<Vec<u8>>,
    seen: Mutex<Vec<Seen>>,
}

impl RouteHttp {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Answers each `(target, JSON body)` route.
    pub(crate) fn routes(routes: Vec<(&str, serde_json::Value)>) -> Self {
        routes
            .into_iter()
            .fold(Self::new(), |http, (target, body)| http.json(target, body))
    }

    /// Answers every request with the JSON `body`.
    pub(crate) fn always(body: serde_json::Value) -> Self {
        Self::new().otherwise(serde_json::to_vec(&body).expect("test body"))
    }

    /// Answers `target` with the JSON `body`.
    pub(crate) fn json(self, target: &str, body: serde_json::Value) -> Self {
        self.bytes(target, serde_json::to_vec(&body).expect("test body"))
    }

    /// Answers `target` requested as `accept` with the JSON `body`.
    pub(crate) fn json_for(
        mut self,
        target: &str,
        accept: &'static str,
        body: serde_json::Value,
    ) -> Self {
        self.routes.push(Route {
            target: target.to_owned(),
            accept: Some(accept),
            status: 200,
            body: serde_json::to_vec(&body).expect("test body"),
        });
        self
    }

    /// Answers `target` with the raw `body`.
    pub(crate) fn bytes(mut self, target: &str, body: impl Into<Vec<u8>>) -> Self {
        self.routes.push(Route {
            target: target.to_owned(),
            accept: None,
            status: 200,
            body: body.into(),
        });
        self
    }

    /// Answers `target` with HTTP `status` and the JSON `body`.
    pub(crate) fn status(mut self, target: &str, status: u16, body: serde_json::Value) -> Self {
        self.routes.push(Route {
            target: target.to_owned(),
            accept: None,
            status,
            body: serde_json::to_vec(&body).expect("test body"),
        });
        self
    }

    /// Answers every unrouted request with `body` instead of a 404.
    pub(crate) fn otherwise(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.otherwise = Some(body.into());
        self
    }

    /// Every request received, in arrival order.
    pub(crate) fn seen(&self) -> Vec<Seen> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// The `path?query` of every request received, in arrival order.
    pub(crate) fn targets(&self) -> Vec<String> {
        self.seen().iter().map(Seen::target).collect()
    }
}

impl ArtifactHttp for RouteHttp {
    fn get<'a>(
        &'a self,
        req: ArtifactHttpRequest,
        _budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        let seen = Seen {
            url: req.url.clone(),
            accept: req.accept,
            authorized: req.authorization.is_some(),
        };
        let (target, path) = (seen.target(), req.url.path().to_owned());
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(seen);
        let matches = |route: &&Route, wanted: &str| {
            route.target == wanted && route.accept.is_none_or(|accept| accept == req.accept)
        };
        let answer = self
            .routes
            .iter()
            .rev()
            .find(|route| matches(route, &target))
            .or_else(|| self.routes.iter().rev().find(|route| matches(route, &path)))
            .map(|route| (route.status, route.body.clone()))
            .or_else(|| self.otherwise.clone().map(|body| (200, body)));
        Box::pin(async move {
            Ok(match answer {
                Some((status, body)) => ArtifactHttpResponse { status, body },
                None => ArtifactHttpResponse {
                    status: 404,
                    body: b"{}".to_vec(),
                },
            })
        })
    }
}

/// A registry no test may reach: any request fails the test.
pub(crate) struct NoHttp;

impl ArtifactHttp for NoHttp {
    fn get<'a>(
        &'a self,
        req: ArtifactHttpRequest,
        _budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        panic!("this test issues no registry request: {}", req.url)
    }
}

/// Answers the n-th request with the n-th body (status 200); the last body
/// answers every request after it. Counts the requests.
pub(crate) struct SequenceHttp {
    bodies: Vec<Vec<u8>>,
    calls: AtomicUsize,
}

impl SequenceHttp {
    pub(crate) fn new(bodies: Vec<Vec<u8>>) -> Self {
        assert!(!bodies.is_empty(), "a sequence answers with some body");
        Self {
            bodies,
            calls: AtomicUsize::new(0),
        }
    }

    /// Requests received.
    pub(crate) fn calls(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

impl ArtifactHttp for SequenceHttp {
    fn get<'a>(
        &'a self,
        _req: ArtifactHttpRequest,
        _budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        let call = self.calls.fetch_add(1, Ordering::Relaxed);
        let body = self.bodies[call.min(self.bodies.len() - 1)].clone();
        Box::pin(async move { Ok(ArtifactHttpResponse { status: 200, body }) })
    }
}

/// Answers every request with `body` (status 200); a streamed read gets it
/// in `chunk`-byte pieces. Counts the pieces streamed reads took.
pub(crate) struct ChunkedHttp {
    body: Vec<u8>,
    chunk: usize,
    fed: AtomicUsize,
}

impl ChunkedHttp {
    pub(crate) fn new(body: Vec<u8>, chunk: usize) -> Self {
        Self {
            body,
            chunk,
            fed: AtomicUsize::new(0),
        }
    }

    /// Pieces streamed reads took.
    pub(crate) fn fed(&self) -> usize {
        self.fed.load(Ordering::Relaxed)
    }

    /// Pieces the whole body is.
    pub(crate) fn pieces(&self) -> usize {
        self.body.len().div_ceil(self.chunk)
    }
}

impl ArtifactHttp for ChunkedHttp {
    fn get<'a>(
        &'a self,
        _req: ArtifactHttpRequest,
        _budget: &'a RequestBudget,
    ) -> ArtifactHttpFuture<'a> {
        let body = self.body.clone();
        Box::pin(async move { Ok(ArtifactHttpResponse { status: 200, body }) })
    }

    fn stream<'a>(
        &'a self,
        _req: ArtifactHttpRequest,
        _budget: &'a RequestBudget,
        sink: BodySink<'a>,
    ) -> StatusFuture<'a> {
        Box::pin(async move {
            for piece in self.body.chunks(self.chunk) {
                self.fed.fetch_add(1, Ordering::Relaxed);
                if sink(piece).is_break() {
                    break;
                }
            }
            Ok(200)
        })
    }
}

/// GitHub's answer to every release-ref check: the refs that exist
/// (`owner/repo@ref`), or one fixed answer (`None`: the check could not run).
/// Records every check.
pub(crate) struct KnownTags {
    answer: Answer,
    seen: Mutex<Vec<String>>,
}

enum Answer {
    Known(Vec<&'static str>),
    Always(Option<bool>),
}

impl KnownTags {
    pub(crate) fn new(known: Vec<&'static str>) -> Self {
        Self {
            answer: Answer::Known(known),
            seen: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn always(exists: Option<bool>) -> Self {
        Self {
            answer: Answer::Always(exists),
            seen: Mutex::new(Vec::new()),
        }
    }

    /// Every check, as `owner/repo@ref`, in order.
    pub(crate) fn seen(&self) -> Vec<String> {
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

impl ReleaseTags for KnownTags {
    fn tag_exists<'a>(&'a self, owner: &'a str, repo: &'a str, tag: &'a str) -> TagFuture<'a> {
        self.known(owner, repo, tag)
    }

    fn commit_exists<'a>(
        &'a self,
        owner: &'a str,
        repo: &'a str,
        commit: &'a str,
    ) -> TagFuture<'a> {
        self.known(owner, repo, commit)
    }
}

impl KnownTags {
    fn known<'a>(&'a self, owner: &'a str, repo: &'a str, reference: &'a str) -> TagFuture<'a> {
        let name = format!("{owner}/{repo}@{reference}");
        let exists = match &self.answer {
            Answer::Known(known) => Some(known.contains(&name.as_str())),
            Answer::Always(exists) => *exists,
        };
        self.seen
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(name);
        Box::pin(async move { exists })
    }
}
