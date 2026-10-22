//! wiremock mounts and responders shared by every crate's HTTP tests: the
//! github crate's own tests, and octocode-native's unit and integration tests
//! through the `test-support` feature (enabled only by dev builds).
//!
//! Every responder logs each request's arrival, so a test can count hits or
//! check request spacing without a responder of its own.
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Answer `GET route` on `server` with `status` and a JSON `body`.
pub async fn mount_json(
    server: &MockServer,
    route: impl Into<String>,
    status: u16,
    body: impl serde::Serialize,
) {
    Mock::given(method("GET"))
        .and(path(route))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}

/// When each request reached a responder, in arrival order. Clones share one log.
#[derive(Clone, Default)]
pub struct Arrivals(Arc<Mutex<Vec<Instant>>>);

impl Arrivals {
    fn record(&self) -> usize {
        let mut log = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        log.push(Instant::now());
        log.len() - 1
    }

    /// The arrival instants so far.
    pub fn times(&self) -> Vec<Instant> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Requests answered so far.
    pub fn count(&self) -> usize {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).len()
    }
}

/// Answers the n-th request with the n-th template; the last template answers
/// every request after it (`Sequence::new(vec![failure, success])` fails once).
#[derive(Clone)]
pub struct Sequence {
    pub templates: Vec<ResponseTemplate>,
    pub arrivals: Arrivals,
}

impl Sequence {
    pub fn new(templates: Vec<ResponseTemplate>) -> Self {
        assert!(!templates.is_empty(), "a sequence needs a template");
        Self {
            templates,
            arrivals: Arrivals::default(),
        }
    }
}

impl Respond for Sequence {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let index = self.arrivals.record().min(self.templates.len() - 1);
        self.templates[index].clone()
    }
}

/// Answers every request with `template` once `after` has passed. With
/// `arrived`, it also wakes that notify as each request arrives, so a test can
/// act while the answer is still in flight instead of after a guessed sleep.
#[derive(Clone)]
pub struct Delayed {
    pub after: Duration,
    pub template: ResponseTemplate,
    pub arrivals: Arrivals,
    pub arrived: Option<Arc<tokio::sync::Notify>>,
}

impl Delayed {
    pub fn new(after: Duration, template: ResponseTemplate) -> Self {
        Self {
            after,
            template,
            arrivals: Arrivals::default(),
            arrived: None,
        }
    }
}

impl Respond for Delayed {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.arrivals.record();
        if let Some(arrived) = &self.arrived {
            arrived.notify_one();
        }
        self.template.clone().set_delay(self.after)
    }
}
