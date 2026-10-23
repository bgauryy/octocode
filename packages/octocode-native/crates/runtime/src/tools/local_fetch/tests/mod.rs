use super::*;
use crate::execution::cancel::{CancellationCheck, NeverCancel};
use crate::security::scan::ContentScan;
use crate::tools::source_view::*;
use crate::tools::test_support::Fixture;
use octocode_contract_types::ErrorCode;
use sha2::Digest;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
pub(super) struct Paths(pub PathBuf);
impl PathAccess for Paths {
    fn validate_read(&self, p: &Path) -> Result<ValidatedRead, Box<PathFailure>> {
        let p = if p.is_absolute() {
            p.into()
        } else {
            self.0.join(p)
        };
        p.canonicalize()
            .map(|canonical| ValidatedRead {
                // As the workspace policy names it: relative to the root
                // when inside it.
                named: self
                    .0
                    .canonicalize()
                    .ok()
                    .and_then(|root| canonical.strip_prefix(root).ok().map(Path::to_path_buf))
                    .unwrap_or_else(|| canonical.clone())
                    .to_string_lossy()
                    .into_owned(),
                canonical,
                target: None,
                display: p.to_string_lossy().into_owned(),
            })
            .map_err(|error| {
                Box::new(PathFailure {
                    code: ErrorCode::FileAccessFailed,
                    message: error.to_string(),
                    safe_path: None,
                    resource_missing: false,
                    sparse_checkout: false,
                    not_a_file: None,
                    search_root: None,
                    view_tree: None,
                    corrected: None,
                })
            })
    }
}
struct Safe;
impl ContentScan for Safe {
    fn sanitize(&self, text: &str, _: &Path) -> Result<(String, Vec<String>), (ErrorCode, String)> {
        Ok((
            text.replace("SECRET", "[REDACTED]"),
            if text.contains("SECRET") {
                vec!["Secrets detected and redacted: synthetic".into()]
            } else {
                vec![]
            },
        ))
    }
}
/// Read `req` under `paths` with the redacting test scan, no cancellation,
/// default regex and no response window.
fn fetch(req: &LocalFetchQuery, paths: &impl PathAccess) -> LocalFetchResult {
    fetch_with(req, paths, &Safe)
}

/// [`fetch`] under the content scan `scan`.
pub(super) fn fetch_with(
    req: &LocalFetchQuery,
    paths: &impl PathAccess,
    scan: &impl crate::security::scan::ContentScan,
) -> LocalFetchResult {
    execute_local_fetch(req, paths, scan, &NeverCancel, None, None)
}

pub(super) fn q(path: &Path) -> LocalFetchQuery {
    LocalFetchQuery {
        path: path.to_string_lossy().parse().expect("path"),
        ..test_local_fetch_query()
    }
}
fn numbered(n: usize) -> String {
    (1..=n).map(|i| format!("l{i}\n")).collect()
}
struct Cancel(AtomicUsize);
impl CancellationCheck for Cancel {
    fn check(&self) -> Result<(), String> {
        if self.0.fetch_add(1, Ordering::SeqCst) > 0 {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }
}
/// A query built from wire JSON (fields the test does not name default).
fn qj(path: &Path, fields: serde_json::Value) -> LocalFetchQuery {
    let mut query = serde_json::json!({"path": path.to_string_lossy(), "mainGoal": "test", "reasoning": "test"});
    for (key, value) in fields.as_object().expect("object") {
        query[key] = value.clone();
    }
    serde_json::from_value(query).expect("localFetch query")
}

fn workspace_policy(root: &Path) -> crate::policy::path::PathPolicy {
    crate::policy::path::PathPolicy::new(crate::policy::path::PathPolicyConfig {
        workspace_root: Some(root.to_path_buf()),
        ..Default::default()
    })
    .expect("policy")
}

/// `a.txt` of `lines` numbered lines where each line in `hits` reads
/// `hit<n>`, and a query matching `hit` with one context line.
fn hit_lines(t: &Fixture, lines: usize, hits: &[usize]) -> (Paths, LocalFetchQuery) {
    let p = t.0.join("a.txt");
    let text = hits.iter().fold(numbered(lines), |text, hit| {
        text.replace(&format!("l{hit}\n"), &format!("hit{hit}\n"))
    });
    fs::write(&p, text).expect("test fixture operation should succeed");
    let mut req = q(&p);
    req.match_string = Some(MatchString::String("hit".parse().expect("match string")));
    req.context_lines = Some(1);
    (Paths(t.0.clone()), req)
}

const PY_TWO: &str = "import os\n\ndef first(a):\n    x = 1\n    return a\n\n\ndef second(b):\n    if b:\n        return first(1)\n    return 2\n";

mod classified;
mod declarations;
mod line_walks;
mod long_lines;
mod match_misses;
mod match_patterns;
mod match_ranges;
mod match_windows;
mod paging;
mod paths;
mod redaction;
mod regex_dialects;
mod source;
mod streamed;
mod symbols;
mod views;
mod wire;

/// A cold symbols view parses its source once: the outline and the
/// declarations its spans read come from one parse, which seeds the shared
/// declarations cache (a later reader finds it there).
#[test]
fn a_cold_symbols_view_seeds_the_declarations_cache_from_its_own_parse() {
    let dir = tempfile::tempdir().expect("dir");
    let file = dir.path().join(format!("seeded_{}.rs", std::process::id()));
    let source = "pub fn first(value: usize) -> usize {\n    let doubled = value * 2;\n    doubled + 1\n}\n\npub struct Point;\n\nimpl Point {\n    pub fn origin() -> Self {\n        Point\n    }\n}\n";
    std::fs::write(&file, source).expect("source");
    let query = qj(&file, serde_json::json!({"minify": "symbols"}));
    let out = fetch(&query, &workspace_policy(dir.path()));
    assert!(out.error.is_none(), "{out:?}");
    let path = file.to_string_lossy();
    assert!(
        crate::tools::declarations_cache::document(source, &path, None, || {
            panic!("the symbols view's parse is cached")
        })
        .is_some()
    );
}
