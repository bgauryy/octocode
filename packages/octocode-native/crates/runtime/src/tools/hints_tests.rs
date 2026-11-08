//! Hint ownership guard. Every prose template a tool can put under `hints`
//! (directly, or as a notice that lands there on an empty or failed row) is
//! listed once in `hints_tests/registry.tsv` with its owner, so a new hint
//! names its owner and a removed one leaves no stale row. One situation has
//! one wording: an `errorCode` gets the shared recovery on every tool unless
//! [`OVERRIDES`] names the tool and why its own text (or none) fits better.

use std::collections::{BTreeMap, BTreeSet};

use crate::id::{ToolId, error_codes};
use serde_json::Value;

mod scan;

/// A tool whose recovery for an `errorCode` differs from the one every
/// other tool gives, and why. An empty text (`NO_HINT`) is an override too:
/// the row's message and leads already say it all.
/// One `tool code: reason` per line.
const OVERRIDES: &str = "\
astSearch fileTooLarge: the row's own hint and lead name only readers that serve the file's length
astSearch invalidPattern: names the pattern/rule shape and the syntaxTree check
astSearch languageUnsupported: names when parser applies, beside the text fallbacks
astSearch symbolsUnsupported: routes documents to localFetch view:symbols
ghGetHistoryItem notFound: the message names the repository, number or refs
ghSearchRepo notFound: the message names the login; findRepository is the lead
localFetch fileAccessFailed: a read names one path: list it as a file first
localFetch invalidPagination: the message names the bad offset
localSearch fileAccessFailed: no file under the path was readable: broadening reads the same files
localSearch invalidPattern: names the regexEngine:literal repair for literal text
lspSearch anchorUnresolved: findSimilar/read leads and per-call notes say it all
lspSearch capabilityUnavailable: names the advertised operations, or type hierarchy
lspSearch missingConfiguration: names the server's project files (clangd compile database)
lspSearch requestFailed: names the retryable flag and the source fallback
lspSearch serverCrashed: a crash restarts the server; a memory kill repeats
lspSearch serverUnavailable: depends on operation and path; names the server setup
lspSearch timeout: the first call cold-starts the language server
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
                let text =
                    crate::response::rows::error_code_hint(tool.output(), code, &Value::Null)
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

/// The tool a source path belongs to: `tools/<snake>` names it, when that
/// is a tool; everything else is shared.
fn path_owner(file: &str) -> Option<&'static str> {
    let module = file.strip_prefix("tools/")?.split(['/', '.']).next()?;
    let mut camel = String::new();
    for (index, part) in module.split('_').enumerate() {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            if index == 0 {
                camel.push(first);
            } else {
                camel.push(first.to_ascii_uppercase());
            }
            camel.push_str(chars.as_str());
        }
    }
    ToolId::from_name(&camel).map(ToolId::as_str)
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

const REGISTRY: &str = include_str!("hints_tests/registry.tsv");

#[test]
fn every_hint_template_is_registered_once_with_its_owner() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let found = scan::templates(&root);
    let registry = scan::registry(REGISTRY);
    if std::env::var_os("OCTOCODE_WRITE_HINT_REGISTRY").is_some() {
        let mut out = String::from(REGISTRY_HEADER);
        for template in &found {
            let owner = registry.get(template).map_or_else(
                || path_owner(&template.file).unwrap_or("shared").to_owned(),
                Clone::clone,
            );
            let text = template.text.replace('\n', "\\n");
            out.push_str(&format!("{owner}\t{}\t{text}\n", template.file));
        }
        std::fs::write(root.join("tools/hints_tests/registry.tsv"), out).expect("write registry");
        return;
    }
    let unregistered = found
        .iter()
        .filter(|t| !registry.contains_key(t))
        .collect::<Vec<_>>();
    let stale = registry
        .keys()
        .filter(|t| !found.contains(t))
        .collect::<Vec<_>>();
    assert!(
        unregistered.is_empty() && stale.is_empty(),
        "every hint or notice template has one registry row (tools/hints_tests/registry.tsv: \
         owner, file, template); rerun with OCTOCODE_WRITE_HINT_REGISTRY=1 and review the owner \
         of each new row.\nunregistered: {unregistered:#?}\nstale: {stale:#?}"
    );
    for (template, owner) in &registry {
        let known = owner == "shared" || ToolId::from_name(owner).is_some();
        assert!(known, "{owner} is not a tool or `shared`: {template:?}");
        if let Some(tool) = path_owner(&template.file) {
            assert_eq!(
                owner, tool,
                "a hint in {}'s module belongs to it",
                template.file
            );
        }
    }
    let mut sites = BTreeMap::<String, Vec<&scan::Template>>::new();
    for template in registry
        .keys()
        .filter(|t| wording(&t.text).split(' ').count() > 3)
    {
        sites
            .entry(wording(&template.text))
            .or_default()
            .push(template);
    }
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

const REGISTRY_HEADER: &str = "# Every hint and notice template the runtime emits: owner (a tool, or\n\
# shared), source file under src/, template ({…} are format! slots, \\n a\n\
# newline). Checked by tools/hints_tests.rs; regenerate rows with\n\
# OCTOCODE_WRITE_HINT_REGISTRY=1 and review each new owner.\n";

const RAW_WRITES: &str = include_str!("hints_tests/raw_writes.tsv");

#[test]
fn hints_partial_and_continue_are_written_only_by_their_owners() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let found = scan::raw_writes(&root);
    let registry = scan::raw_registry(RAW_WRITES);
    if std::env::var_os("OCTOCODE_WRITE_HINT_REGISTRY").is_some() {
        let mut out = String::from(RAW_WRITES_HEADER);
        for write in &found {
            let reason = registry.get(write).map_or(
                "TODO: route through the owner or say why not",
                String::as_str,
            );
            out.push_str(&format!("{}\t{}\t{reason}\n", write.file, write.field));
        }
        std::fs::write(root.join("tools/hints_tests/raw_writes.tsv"), out).expect("write");
        return;
    }
    let unregistered = found
        .iter()
        .filter(|w| !registry.contains_key(w))
        .collect::<Vec<_>>();
    let stale = registry
        .keys()
        .filter(|w| !found.contains(w))
        .collect::<Vec<_>>();
    assert!(
        unregistered.is_empty() && stale.is_empty(),
        "hints, isPartial and \"kind\":\"continue\" are written by their owners in response/ \
         (add_hints/prepend_hints/set_hints, mark_partial/terminal_limit, Continuation): route \
         the write there, or register it in tools/hints_tests/raw_writes.tsv with its reason \
         (OCTOCODE_WRITE_HINT_REGISTRY=1 lists the rows).\nunregistered: {unregistered:#?}\n\
         stale: {stale:#?}"
    );
    for (write, reason) in &registry {
        assert!(
            !reason.trim().is_empty() && !reason.starts_with("TODO"),
            "{write:?}: a registered raw write says why it is not routed"
        );
    }
}

const RAW_WRITES_HEADER: &str = "# Raw writes of hints, isPartial or \"kind\":\"continue\" outside\n\
# response/, the owner (add_hints, mark_partial, Continuation): file, field,\n\
# why it is not routed. Checked by tools/hints_tests.rs.\n";
