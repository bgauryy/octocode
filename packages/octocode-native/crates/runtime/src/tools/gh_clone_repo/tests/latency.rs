//! Clone latency: the warm checks run together, a fresh checkout spends no separate config calls, and (an open defect, ignored) git setup overlaps the commit pin.
//!
//! Overlap is shown with sync points, not timing: a held git step is
//! released only by the event that overlapping code produces while it runs
//! (a second warm check starting, the pin request reaching the API). Serial
//! code never produces it, so the step's wait ends at [`UNRELEASED`] and the
//! test fails; a passing run never waits for it.
use super::bench::*;
use super::*;
use std::sync::Condvar;

/// How long a held step waits for an event serial code never sends: it
/// bounds only the failing run, so a serial regression fails instead of
/// hanging, and stays inside the clone deadline [`setup`] sets.
const UNRELEASED: Duration = Duration::from_secs(10);

/// The reads `inspect_checkout` runs on a reused checkout.
const WARM_CHECKS: [&str; 5] = [
    "check checkout cleanliness",
    "read checkout HEAD",
    "check local checkout refs",
    "list credential-named files",
    "list checkout symlinks",
];

const AVAILABILITY: &str = "check Git availability";

/// What the held steps saw.
#[derive(Default)]
struct Events {
    /// Hold each warm check until a second one has started.
    hold_warm: bool,
    warm_started: usize,
    /// Per held warm check: released by a peer starting (`true`) or by
    /// [`UNRELEASED`] (`false`).
    warm_released_by_peer: Vec<bool>,
    /// Hold the availability check until the pin request reaches the API.
    hold_availability: bool,
    pin_arrived: bool,
    /// Per held availability check: whether the pin request arrived before
    /// it ended.
    pin_arrived_during_setup: Vec<bool>,
}

#[derive(Default)]
struct SyncPoints {
    events: Mutex<Events>,
    changed: Condvar,
}

impl SyncPoints {
    fn events(&self) -> std::sync::MutexGuard<'_, Events> {
        self.events.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Wait, at most [`UNRELEASED`], until `released` holds; whether it did.
    fn wait_until(
        &self,
        released: impl Fn(&Events) -> bool,
    ) -> (bool, std::sync::MutexGuard<'_, Events>) {
        let (events, _) = self
            .changed
            .wait_timeout_while(self.events(), UNRELEASED, |events| !released(events))
            .unwrap_or_else(PoisonError::into_inner);
        (released(&events), events)
    }

    fn pin_arrived(&self) {
        self.events().pin_arrived = true;
        self.changed.notify_all();
    }

    /// Hold `label` at its sync point, if it has one, before it runs.
    fn before(&self, label: &str) {
        let mut events = self.events();
        if events.hold_warm && WARM_CHECKS.contains(&label) {
            events.warm_started += 1;
            drop(events);
            self.changed.notify_all();
            let (by_peer, mut events) = self.wait_until(|events| events.warm_started >= 2);
            events.warm_released_by_peer.push(by_peer);
            if !by_peer {
                // Serial code: let the rest run rather than wait for each.
                events.hold_warm = false;
            }
        } else if events.hold_availability && label == AVAILABILITY {
            drop(events);
            let (arrived, mut events) = self.wait_until(|events| events.pin_arrived);
            events.pin_arrived_during_setup.push(arrived);
        }
    }
}

/// A runner that holds git steps at their [`SyncPoints`] and then runs them
/// on `inner`.
struct Held<'a> {
    inner: &'a RewriteRunner,
    sync: Arc<SyncPoints>,
}

impl GitRunner for Held<'_> {
    fn run(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
    ) -> Result<GitOutput, CloneError> {
        self.sync.before(&request.label);
        self.inner.run(request, control)
    }

    fn run_nul_records(
        &self,
        request: &GitRunRequest<'_>,
        control: &GitRunControl<'_>,
        record: &mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send),
    ) -> Result<GitOutput, CloneError> {
        self.sync.before(&request.label);
        self.inner.run_nul_records(request, control, record)
    }
}

/// The commit answer, marking at the sync points that the pin request
/// reached the API.
struct SignalsArrival {
    sync: Arc<SyncPoints>,
    answer: wiremock::ResponseTemplate,
}

impl wiremock::Respond for SignalsArrival {
    fn respond(&self, _: &wiremock::Request) -> wiremock::ResponseTemplate {
        self.sync.pin_arrived();
        self.answer.clone()
    }
}

/// The warm path's independent checks (cleanliness, HEAD, local refs, the
/// credential-named files, symlinks) read one checkout and none depends on
/// another: they run together, not one after the other. Each is held until
/// a second one starts, which only concurrent checks can do.
#[test]
fn the_warm_checks_run_together() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let sync = Arc::new(SyncPoints::default());
    let held = Held {
        inner: &runner,
        sync: Arc::clone(&sync),
    };
    let config = CloneConfig::persistent(&home);
    let context = setup(&held, &config, &endpoint, &policy, &NeverCancel, None);
    clone_pinned(&fixture, &query(), &context).expect("first clone");

    sync.events().hold_warm = true;
    let again = clone_pinned(&fixture, &query(), &context).expect("reused clone");
    assert!(again.location.reused);
    assert_eq!(
        sync.events().warm_released_by_peer,
        [true; WARM_CHECKS.len()],
        "the warm checks ran one at a time"
    );
}

/// The partial-clone setup is the fetch's own: `fetch --filter` registers
/// the promisor remote and its filter, so no separate `config` call runs and
/// the checkout still holds both.
#[test]
fn a_fresh_checkout_spends_no_separate_config_calls() {
    let fixture = Fixture::new();
    let (root, home) = home_root();
    let policy = test_support::workspace_policy(&root.0);
    let endpoint = api_github_endpoint();
    let runner = fixture_runner(&fixture);
    let config = CloneConfig::persistent(&home);
    let context = setup(&runner, &config, &endpoint, &policy, &NeverCancel, None);
    let result = clone_pinned(&fixture, &query(), &context).expect("clone");
    let seen = runner
        .seen
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert!(
        !seen.iter().any(|call| call.contains("configure sparse")),
        "{seen:?}"
    );
    let dir = Path::new(&result.location.local_path);
    assert_eq!(
        git_output(dir, &["config", "--get", "remote.origin.promisor"]).trim(),
        "true"
    );
    assert_eq!(
        git_output(
            dir,
            &["config", "--get", "remote.origin.partialclonefilter"]
        )
        .trim(),
        "blob:none"
    );
}

/// Git's availability and the clone root are checked while the API still
/// answers the pin, not after it: the availability check is held until the
/// pin request reaches the API, which only happens while it runs when the
/// two overlap.
///
/// Open defect (B38, P7 report): `run` joins the pin with a blocking
/// `prepare_clone` on the same task, so on a single-threaded runtime (the CLI
/// and `octocode mcp`) the request is not sent until the setup returns. Making
/// the overlap real needs the setup off the runtime thread, which needs an
/// owned `CloneContext` runner and cancellation.
#[ignore = "B38 open defect: the pin request is not sent while git setup runs"]
#[tokio::test]
async fn git_setup_overlaps_the_pin_request() {
    use wiremock::matchers::{method, path};
    let server = wiremock::MockServer::builder().start().await;
    let bench = Bench::new();
    let provider = one_shot_provider(&server);
    let sync = Arc::new(SyncPoints::default());
    wiremock::Mock::given(method("GET"))
        .and(path(COMMITS_MAIN))
        .respond_with(SignalsArrival {
            sync: Arc::clone(&sync),
            answer: commit(&bench.fixture.first_commit),
        })
        .mount(&server)
        .await;
    let held = Held {
        inner: &bench.runner,
        sync: Arc::clone(&sync),
    };
    sync.events().hold_availability = true;
    let context = setup(
        &held,
        &bench.config,
        &bench.endpoint,
        &bench.policy,
        &NeverCancel,
        None,
    );
    let request = RequestContext::new(
        octocode_io::budget::RequestBudget::with_timeout(Duration::from_secs(60), 1 << 20),
        None,
    );
    run(&provider, &query(), Ok(&request), &context)
        .await
        .unwrap_or_else(|_| panic!("clone"));
    assert_eq!(
        sync.events().pin_arrived_during_setup,
        [true],
        "git setup ran before the pin request was sent"
    );
}
