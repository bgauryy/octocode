//! Rust coverage: cfg-gated files that mention the symbol, checked among
//! the request's text scan, bounded by its cancellation or deadline.
use super::*;

pub(super) fn annotate_rust(
    row: &mut Value,
    query: &LspSearchQuery,
    server_root: &str,
    scope: &Scope,
    all_features: bool,
    stopped: &dyn Fn() -> bool,
) {
    let Some(name) = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return;
    };
    if all_features {
        let negated =
            gated_files_mentioning(scope, server_root, name, Gate::NegatedFeature, stopped);
        if negated.matches > 0 || negated.incomplete() {
            let warning = format!(
                "{} Rust files that mention `{name}` carry cfg(not(feature …)) gates; an all-features build compiles that code out. {}Compare with the default-features run, or confirm with {}.",
                negated.matches,
                negated.incomplete_warning(),
                text_check()
            );
            flag_partial(row, query, CFG_NEGATED_REASON, Some(&warning), scope);
        }
        return;
    }
    let gated = gated_files_mentioning(scope, server_root, name, Gate::Feature, stopped);
    if gated.matches == 0 && !gated.incomplete() {
        return;
    }
    // Gated files found: the rerun page and the partial reason say it all.
    // Only an incomplete scan that found none needs the prose.
    if gated.matches > 0 {
        flag_partial(row, query, CFG_REASON, None, scope);
    } else {
        let warning = format!(
            "{}Uses under features rust-analyzer left off may be missing: {}.",
            gated.incomplete_warning(),
            text_check()
        );
        flag_partial(row, query, CFG_REASON, None, scope);
        // The caveat reads the same on every row of the scan: said once per
        // response (a notice), beside each row's own reason.
        if flagged_with(row, &[CFG_REASON]) {
            crate::response::paging::push_notice(row, warning);
        }
        return;
    }
    // The rerun repeats the whole walk under every feature: offered once,
    // on the walk's first page.
    if continues_a_page(query) {
        return;
    }
    crate::response::result::attach_related(
        row,
        super::super::failure::next_action(query, CFG_REASON),
    );
}

/// Which `cfg` gate a scan looks for.
#[derive(Clone, Copy, Debug)]
pub(super) enum Gate {
    /// `cfg(… feature …)`: compiled out under the default features.
    Feature,
    /// `cfg(not(feature …))`: compiled out under all features.
    NegatedFeature,
}

impl Gate {
    pub(super) fn carried_by(self, text: &str) -> bool {
        match self {
            Self::Feature => has_feature_gate(text),
            Self::NegatedFeature => has_negated_feature_gate(text),
        }
    }
}

/// Rust files under the server root that use a name in code (not only in
/// a comment or string), carry a gate and are missing from the answer, and
/// whether the check was complete.
#[derive(Clone, Debug, Default)]
pub(super) struct GateScan {
    /// Canonical paths.
    pub(super) files: Vec<String>,
    pub(super) has_more: bool,
    pub(super) unreadable: usize,
}

/// What a gate check counts: gated files the server's answer did not
/// cover (an answered file was analyzed), and whether the check finished.
pub(super) struct GateCount {
    pub(super) matches: usize,
    pub(super) scan: GateScan,
}

impl GateCount {
    pub(super) fn incomplete(&self) -> bool {
        self.scan.has_more || self.scan.unreadable > 0
    }

    pub(super) fn incomplete_warning(&self) -> String {
        if self.incomplete() {
            format!(
                "The gate scan was incomplete (unvisited files: {}, unreadable or oversized files: {}); the count is a lower bound. ",
                if self.scan.has_more {
                    "at least one"
                } else {
                    "none"
                },
                self.scan.unreadable
            )
        } else {
            String::new()
        }
    }
}

/// Gated files under `root` (the server's workspace) that use `name` and
/// that the server's answer did not cover. The candidates are the request's
/// own text scan of `name` (`Scope::scanned_files`, a word-bounded search of
/// the scope, memoized by its fingerprint) within `root`: no second walk,
/// and only files that spell the name are read. A request without that scan
/// (it failed) checked nothing, which `has_more` says; `stopped` (the
/// request's cancellation or deadline) ends the check early the same way.
pub(super) fn gated_files_mentioning(
    scope: &Scope,
    root: &str,
    name: &str,
    gate: Gate,
    stopped: &dyn Fn() -> bool,
) -> GateCount {
    let mut scan = GateScan::default();
    let Some(mentions) = scope.scanned_files(name) else {
        scan.has_more = true;
        return GateCount { matches: 0, scan };
    };
    let root = super::super::scope::canonical(root);
    for file in mentions.iter().filter(|file| {
        Path::new(file).starts_with(&root) && !scope.is_answered(file) && scope.in_reach(name, file)
    }) {
        if stopped() {
            scan.has_more = true;
            break;
        }
        #[cfg(test)]
        GATE_READS.with(|reads| reads.set(reads.get() + 1));
        let path = Path::new(file);
        // Only a file past the read cap leaves the check incomplete.
        let text = path
            .metadata()
            .is_ok_and(|meta| meta.len() <= MAX_FILE_BYTES)
            .then(|| {
                crate::tools::source::read_bounded(path, crate::num::usize_of(MAX_FILE_BYTES))
                    .ok()
                    .and_then(|bytes| super::super::source::admitted_text(&bytes).ok())
            })
            .flatten();
        match text {
            Some(text) if gate.carried_by(&text) && uses_in_code(path, &text, name) => {
                scan.files.push(file.clone());
            }
            Some(_) => {}
            None => scan.unreadable += 1,
        }
    }
    GateCount {
        matches: scan.files.len(),
        scan,
    }
}

// Files the gate check read on this thread.
#[cfg(test)]
thread_local! {
    pub(super) static GATE_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether `text` uses `name` in code, not only in comments or strings; a
/// file the grammar cannot parse cleanly counts its word mentions.
pub(super) fn uses_in_code(path: &Path, text: &str, name: &str) -> bool {
    octocode_engine::lsp::resolver::code_occurrence_lines(&path.to_string_lossy(), text, name)
        .is_none_or(|lines| !lines.is_empty())
}

pub(super) fn has_feature_gate(text: &str) -> bool {
    text.match_indices("cfg").any(|(at, _)| {
        let rest = &text[at + 3..];
        rest.trim_start().starts_with('(')
            && rest
                .split(')')
                .next()
                .is_some_and(|gate| gate.contains("feature"))
    })
}

/// A `cfg(not(feature …))` gate (also nested, `cfg(all(not(feature …)))`).
pub(super) fn has_negated_feature_gate(text: &str) -> bool {
    text.match_indices("cfg").any(|(at, _)| {
        let rest = text[at + 3..].trim_start();
        rest.starts_with('(')
            && rest
                .split(']')
                .next()
                .is_some_and(|gate| gate.replace(' ', "").contains("not(feature"))
    })
}
