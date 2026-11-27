//! Hint ownership guard. One situation has one wording at one site: every
//! prose template a tool can put under `hints` (directly, or as a notice
//! that lands there on an empty or failed row), found in the production
//! source by [`scan`], says its situation once, and the module it is written
//! in owns it. An `errorCode` gets the shared recovery on every tool unless
//! [`OVERRIDES`] names the tool and why its own text (or none) fits better.

use crate::runtime::tool_output::ToolOutputOf;
use std::collections::{BTreeMap, BTreeSet};

use octocode_runtime_base::id::{ToolId, error_codes};
use serde_json::Value;

mod scan;

/// A tool whose recovery for an `errorCode` differs from the one every
/// other tool gives, and why. An empty text (`NO_HINT`) is an override too:
/// the row's message and leads already say it all.
/// One `tool code: reason` per line.
const OVERRIDES: &str = "\
artifactSearch notFound: names packageName and ecosystem as the registry spells them; the shared hint is about paths
artifactSearch securityValidationFailed: the message names the refused registryUrl host and the setting that permits it
astSearch fileTooLarge: the row's own hint and lead name only readers that serve the file's length
astSearch languageUnsupported: names when parser applies, beside the text fallbacks
ghGetFileContent languageUnsupported: the shared hint names local tools; the related[] plain read is the recovery
ghGetFileContent largeSourceWindowOnly: the shared hint names localSearch and localFetch, which cannot read a GitHub file; the message names the matchString read
ghGetHistoryItem notFound: a missing item is a number or ref, not a path: the shared path-listing hint does not apply
localFetch fileAccessFailed: a read names one path: list it as a file first
localFetch invalidPagination: the message names the bad offset
localSearch fileAccessFailed: no file under the path was readable: broadening reads the same files
lspSearch capabilityUnavailable: names the advertised operations, or type hierarchy
lspSearch missingConfiguration: names the server's project files (clangd compile database)
lspSearch requestFailed: names the retryable flag and the source fallback
lspSearch serverCrashed: a crash restarts the server; a memory kill repeats
lspSearch serverUnavailable: depends on operation and path; names the server setup
lspSearch timeout: the first call cold-starts the language server
securityCheck notFound: names the package, advisory id or repository as its database writes it; the shared hint is about paths
";

/// The `(tool, code, reason)` rows of [`OVERRIDES`].
fn overrides() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    OVERRIDES.lines().map(|line| {
        let (key, reason) = line.split_once(": ").expect("tool code: reason");
        let (tool, code) = key.split_once(' ').expect("tool code");
        (tool, code, reason)
    })
}

/// The recovery text of every tool for every declared code, with no query.
fn recoveries() -> BTreeMap<&'static str, Vec<(ToolId, &'static str)>> {
    let mut by_code = BTreeMap::new();
    for &code in error_codes::ALL {
        let texts = ToolId::ALL
            .into_iter()
            .map(|tool| {
                let text = octocode_runtime_base::response::rows::error_code_hint(
                    tool.output(),
                    code,
                    &Value::Null,
                )
                .unwrap_or_default();
                (tool, text)
            })
            .collect::<Vec<_>>();
        by_code.insert(error_codes::as_str(code), texts);
    }
    by_code
}

/// The text most tools give for a code: the shared recovery.
fn shared(texts: &[(ToolId, &'static str)]) -> &'static str {
    let mut counts = BTreeMap::<&str, usize>::new();
    for (_, text) in texts {
        *counts.entry(text).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map_or("", |(text, _)| text)
}

#[test]
fn one_error_code_has_one_recovery_unless_a_tool_says_why() {
    let mut found = Vec::new();
    for (code, texts) in recoveries() {
        let shared = shared(&texts);
        // A code no shared recovery answers is owned by the tools that emit
        // it; two of them wording it differently still need a reason.
        let owned = texts
            .iter()
            .filter(|(_, text)| !text.is_empty())
            .map(|(_, text)| *text);
        let conflict = shared.is_empty() && owned.collect::<BTreeSet<_>>().len() > 1;
        for (tool, text) in texts {
            if text != shared && (!shared.is_empty() || conflict) {
                found.push((tool.as_str(), code));
            }
        }
    }
    let listed = overrides()
        .map(|(tool, code, _)| (tool, code))
        .collect::<Vec<_>>();
    let unlisted = found
        .iter()
        .filter(|key| !listed.contains(key))
        .collect::<Vec<_>>();
    let stale = listed
        .iter()
        .filter(|key| !found.contains(key))
        .collect::<Vec<_>>();
    assert!(
        unlisted.is_empty() && stale.is_empty(),
        "a tool words a shared errorCode recovery its own way: use the shared hint or add \
         a `tool code: reason` line to OVERRIDES.\nunlisted: {unlisted:?}\nstale: {stale:?}"
    );
    for (tool, code, reason) in overrides() {
        assert!(
            !reason.trim().is_empty(),
            "{tool} {code}: every override says why"
        );
    }
}

/// A template's wording with its placeholders and punctuation dropped, so
/// two templates that say the same thing compare equal.
fn wording(text: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            c if depth == 0 && c.is_alphanumeric() => out.extend(c.to_lowercase()),
            c if depth == 0 && c.is_whitespace() && !out.ends_with(' ') => out.push(' '),
            _ => {}
        }
    }
    out.trim().to_owned()
}

/// The sources whose hints the scans read: every crate of the runtime,
/// `octocode-runtime-base` included, where `response/` owns the hint writers.
fn source_roots() -> Vec<std::path::PathBuf> {
    octocode_runtime_base::test_support::runtime_crate_sources()
}

#[test]
fn one_hint_wording_is_written_at_one_site() {
    let mut sites = BTreeMap::<String, Vec<scan::Template>>::new();
    for template in source_roots().iter().flat_map(|root| scan::templates(root)) {
        let wording = wording(&template.text);
        if wording.split(' ').count() > 3 {
            sites.entry(wording).or_default().push(template);
        }
    }
    assert!(
        !sites.is_empty(),
        "the scan finds the runtime's hint templates"
    );
    let repeated = sites
        .values()
        .filter(|sites| sites.len() > 1)
        .collect::<Vec<_>>();
    assert!(
        repeated.is_empty(),
        "one situation, one wording, one owner: share one constant instead of \
         restating a hint at several sites: {repeated:#?}"
    );
}

#[test]
fn hints_partial_and_continue_are_written_only_by_their_owners() {
    let raw = source_roots()
        .iter()
        .flat_map(|root| scan::raw_writes(root))
        .collect::<Vec<_>>();
    assert!(
        raw.is_empty(),
        "hints, isPartial and \"kind\":\"continue\" are written by their owners in response/ \
         (add_hints/prepend_hints/set_hints, mark_partial/terminal_limit, Continuation): route \
         these writes there: {raw:#?}"
    );
}
