//! Rust coverage: cfg-gated files that mention the symbol, found by a
//! bounded and cached gate scan.
use super::*;

pub(super) fn annotate_rust(
    row: &mut Value,
    query: &LspSearchQuery,
    server_root: &str,
    scope: &Scope,
) {
    let context = lsp_search_query::rust_context(query).unwrap_or_else(|| json!({}));
    let Some(name) = lsp_search_query::symbol_name(query).filter(|name| !name.trim().is_empty())
    else {
        return;
    };
    if context["features"] == "all" {
        let negated = gated_files_mentioning(scope, server_root, name, Gate::NegatedFeature);
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
    let gated = gated_files_mentioning(scope, server_root, name, Gate::Feature);
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
        flag_partial(row, query, CFG_REASON, Some(&warning), scope);
        return;
    }
    // The rerun repeats the whole walk under every feature: offered once,
    // on the walk's first page.
    if continues_a_page(query) {
        return;
    }
    let mut rerun = lsp_search_query::to_row(query);
    if let Some(fields) = rerun.as_object_mut() {
        fields
            .retain(|key, value| !value.is_null() && !matches!(key.as_str(), "page" | "snapshot"));
    }
    let mut context = context;
    context["features"] = json!("all");
    rerun["rustContext"] = context;
    row["next"]["expandFeatures"] =
        crate::response::result::Continuation::new(ToolId::LspSearch, rerun)
            .confidence(Confidence::Medium)
            .build();
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

/// Rust files under the scope that use a name in code (not only in a
/// comment or string) and carry a gate, and whether the scan was complete.
#[derive(Clone, Debug, Default)]
pub(super) struct GateScan {
    /// Canonical paths.
    pub(super) files: Vec<String>,
    pub(super) has_more: bool,
    pub(super) unreadable: usize,
}

/// Gate scans by `(root, name, gate, scope fingerprint)`: the fingerprint
/// covers every Rust file under the root, so an unchanged one proves the
/// same files. Later pages of a walk reuse their first page's scan.
pub(super) static GATE_SCANS: octocode_io::memo::Memo<String, GateScan> =
    octocode_io::memo::Memo::new(octocode_io::memo::MemoLimits {
        max_entries: 256,
        max_weight: 256,
        keep_newest: false,
    });

/// What a gate check counts: gated files the server's answer did not
/// cover (an answered file was analyzed), and whether the scan finished.
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

/// Gated files under `root` (the server's workspace) that use `name`. The
/// scope fingerprint covers every Rust file under the scope, which holds
/// `root`, so it keys the memo.
pub(super) fn gated_files_mentioning(
    scope: &Scope,
    root: &str,
    name: &str,
    gate: Gate,
) -> GateCount {
    let key = scope.fingerprint().map(|fingerprint| {
        octocode_engine::digest::sha256(
            format!("{root}\u{0}{name}\u{0}{gate:?}\u{0}{fingerprint}").as_bytes(),
        )
    });
    let scan = match key.as_ref().and_then(|key| GATE_SCANS.get(key)) {
        Some(hit) => (*hit.value).clone(),
        None => {
            let scan = scan_gates(Path::new(root), name, gate, MAX_SCANNED_FILES);
            if let Some(key) = key {
                GATE_SCANS.insert(key, scan.clone(), 1, None);
            }
            scan
        }
    };
    GateCount {
        matches: scan
            .files
            .iter()
            .filter(|file| !scope.is_answered(file))
            .count(),
        scan,
    }
}

pub(super) fn scan_gates(root: &Path, name: &str, gate: Gate, limit: usize) -> GateScan {
    let walker = crate::tools::pruned_walk(root, []).build();
    let mut scan = GateScan::default();
    let mut visited = 0;
    let started = std::time::Instant::now();
    for entry in walker {
        if started.elapsed() >= GATE_SCAN_BUDGET {
            scan.has_more = true;
            break;
        }
        let Ok(entry) = entry else {
            scan.unreadable += 1;
            continue;
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file())
            || entry.path().extension().is_none_or(|ext| ext != "rs")
        {
            continue;
        }
        if visited >= limit {
            scan.has_more = true;
            break;
        }
        visited += 1;
        // Only a file past the read cap leaves the scan incomplete.
        if !entry
            .metadata()
            .is_ok_and(|meta| meta.len() <= MAX_FILE_BYTES)
        {
            scan.unreadable += 1;
            continue;
        }
        match crate::tools::source::read_bounded(entry.path(), MAX_FILE_BYTES as usize)
            .ok()
            .and_then(|bytes| super::super::source::admitted_text(&bytes).ok())
        {
            Some(text)
                if gate.carried_by(&text)
                    && mentions_word(&text, name)
                    && uses_in_code(entry.path(), &text, name) =>
            {
                scan.files.push(super::super::scope::canonical(
                    &entry.path().to_string_lossy(),
                ));
            }
            Some(_) => {}
            None => scan.unreadable += 1,
        }
    }
    scan
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

pub(super) fn mentions_word(text: &str, name: &str) -> bool {
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    text.match_indices(name).any(|(at, _)| {
        !text[..at].chars().next_back().is_some_and(is_ident)
            && !text[at + name.len()..].chars().next().is_some_and(is_ident)
    })
}
