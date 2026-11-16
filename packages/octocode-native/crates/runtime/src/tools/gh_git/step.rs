//! One git step: hardened arguments, stall bounds and limiter admission around the runner.
use super::admission::git_permit;
use super::{CloneContext, CloneError, GitOutput, GitRunRequest, control};
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

/// The most one git step may run: until the caller's deadline. No step has
/// a wall-clock cap of its own, so a transfer that keeps flowing is never
/// cut; a stalled one is ended by git itself ([`stall_args`]), and
/// cancellation ends any step at once.
pub(crate) fn step_timeout(context: &CloneContext<'_>) -> Duration {
    context
        .deadline
        .saturating_duration_since(std::time::Instant::now())
}

pub(crate) fn scoped(target: &Path, args: &[&str]) -> Vec<OsString> {
    let mut result = vec!["-C".into(), target.as_os_str().to_owned()];
    result.extend(args.iter().map(OsString::from));
    result
}

/// Global options prepended to every git invocation. `core.symlinks=false`
/// makes clone/fetch/checkout materialize repository symlinks as plain files
/// holding the link text, so a hostile repo cannot point into other
/// `~/.octocode` state that localFetch would then read through the clone.
pub(crate) const HARDENING_ARGS: [&str; 2] = ["-c", "core.symlinks=false"];

pub(crate) fn hardened(args: Vec<OsString>) -> Vec<OsString> {
    HARDENING_ARGS
        .iter()
        .map(OsString::from)
        .chain(args)
        .collect()
}

/// Options that make git end an HTTP transfer which moves less than a byte
/// per second for `network_timeout`: the stall bound of every network step.
fn stall_args(network_timeout: Duration) -> Vec<OsString> {
    vec![
        "-c".into(),
        "http.lowSpeedLimit=1".into(),
        "-c".into(),
        format!("http.lowSpeedTime={}", network_timeout.as_secs().max(1)).into(),
    ]
}

pub(crate) fn run(
    context: &CloneContext<'_>,
    args: Vec<OsString>,
    timeout: Duration,
    label: &str,
    authorization_url: Option<&str>,
) -> Result<GitOutput, CloneError> {
    run_with(
        context,
        Step {
            args,
            timeout,
            label,
            authorization_url,
            stdin: None,
        },
        None,
    )
}

/// Where a step's stdout goes, as it arrives.
pub(crate) type ChunkSink<'a> = &'a mut (dyn FnMut(&[u8]) -> Result<(), CloneError> + Send);

/// One git invocation: its arguments, the step's name for errors, the remote
/// whose credential it may use (a network step) and the bytes it reads.
pub(crate) struct Step<'a> {
    pub(crate) args: Vec<OsString>,
    pub(crate) timeout: Duration,
    pub(crate) label: &'a str,
    pub(crate) authorization_url: Option<&'a str>,
    pub(crate) stdin: Option<&'a [u8]>,
}

/// [`run`] for a step with stdin, whose stdout goes to `sink` when given
/// instead of being collected.
pub(crate) fn run_with(
    context: &CloneContext<'_>,
    step: Step<'_>,
    sink: Option<ChunkSink<'_>>,
) -> Result<GitOutput, CloneError> {
    let Step {
        args,
        timeout,
        label,
        authorization_url,
        stdin,
    } = step;
    let mut args = hardened(args);
    if authorization_url.is_some() {
        args.splice(0..0, stall_args(context.config.network_timeout));
    }
    let authorization = context
        .credential
        .map(octocode_github::ResolvedCredential::expose_secret);
    // Network git operations share the host/token executor: honor its
    // secondary-limit cooldown and cap concurrent clones via the `git` group.
    let _git_permit = match authorization_url {
        Some(url) => Some(git_permit(context, url, authorization)?),
        None => None,
    };
    let request = GitRunRequest {
        args,
        timeout,
        label: label.into(),
        authorization,
        authorization_url,
        stdin,
    };
    match sink {
        Some(sink) => context.git.run_stream(&request, &control(context), sink),
        None => context.git.run(&request, &control(context)),
    }
}
