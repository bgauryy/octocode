//! `Importers`: per-file anchors, owned files, coverage digests, settling a
//! silent page, and disclosing ignored or capped scans.
use super::*;

impl Importers {
    /// One anchor per file for location requests (`references`).
    pub(in super::super) fn per_file(&self) -> impl Iterator<Item = &Anchor> {
        let mut seen = HashSet::new();
        self.anchors
            .iter()
            .filter(move |anchor| seen.insert(anchor.path.clone()))
    }

    /// One anchor per file for call hierarchy, preferring a call site.
    pub(in super::super) fn call_sites(&self) -> Vec<&Anchor> {
        let mut chosen: Vec<&Anchor> = Vec::new();
        for anchor in &self.anchors {
            match chosen.iter_mut().find(|kept| kept.path == anchor.path) {
                Some(kept) if anchor.is_call && !kept.is_call => *kept = anchor,
                Some(_) => {}
                None => chosen.push(anchor),
            }
        }
        chosen
    }

    /// Whether a recovered location in `file` (canonical) belongs to the
    /// windows this request verified: a candidate's locations belong to its
    /// own window.
    pub(in super::super) fn owns(&self, file: &str) -> bool {
        self.window
            .as_ref()
            .is_none_or(|window| window.verified_owner(file))
    }

    /// The candidate digest, salted into this request's location snapshot so
    /// a changed candidate list stales the window's later location pages.
    pub(in super::super) fn digest(&self) -> Option<&str> {
        self.window.as_ref().map(|window| window.digest.as_str())
    }

    /// Candidates checked by this window and every earlier one, less this
    /// window's unverified candidates: those stay text-only so the row's
    /// coverage and lexical lead disclose them.
    pub(in super::super) fn covered(&self) -> Vec<String> {
        let unverified = self.unverified.iter().collect::<HashSet<_>>();
        self.window
            .as_ref()
            .map_or(&[][..], Window::covered)
            .iter()
            .filter(|file| !unverified.contains(file))
            .cloned()
            .collect()
    }

    /// A page whose windows held no verified importer lists no site. While
    /// the scan goes on (or after earlier windows), that is one step of the
    /// scan, not an empty answer: the row stops being `empty` (no exit-1
    /// outcome, no re-anchor hint) and says which candidates were checked
    /// and whether the scan goes on. Only a first page that ends the scan
    /// with no site is a true empty answer.
    pub(super) fn settle_silent_page(
        &self,
        row: &mut Value,
        query: &LspSearchQuery,
        window: &Window,
    ) {
        if (window.first <= 1 && !window.more())
            || row.get("status").and_then(Value::as_str) != Some("empty")
        {
            return;
        }
        let (from, to) = window.verified_candidates();
        let held = if self.anchors.is_empty() {
            "held no reference to this symbol"
        } else {
            "held no reference to this symbol that an earlier page does not list"
        };
        let tail = match (window.more(), self.failed) {
            (true, _) => "the scan continues: the importer lead verifies the following candidates",
            // Candidates that could not be read are not verified: the scan
            // ended, but it is not complete (the warning names the failure).
            (false, true) => {
                "that was the last window; the candidates the scan could not read stay unchecked"
            }
            (false, false) => "that was the last window, so the importer scan is complete",
        };
        let Some(object) = row.as_object_mut() else {
            return;
        };
        for field in ["status", "category", "hints"] {
            object.remove(field);
        }
        object.insert("kind".into(), json!(lsp_search_query::operation(query)));
        object.insert("matches".into(), json!([]));
        object.insert(
            "reason".into(),
            json!(format!(
                "Candidates {from}-{to} of {} {held}; {tail}.",
                window.total
            )),
        );
    }

    /// Gitignored files are outside the candidate scan. One that imports the
    /// name may be a hidden importer, and one the import check could not
    /// decide is not ruled out: the row is partial, counts each kind, names
    /// the deciding rules, and leads to exactly those files. Files that only
    /// spell the name are no evidence of a hidden importer: the row keeps
    /// only the lead over them. A scan that stopped at its bound left the
    /// rest unverified: its lead is over the scope, which holds the files it
    /// found and those it did not reach, and without evidence the row is
    /// partial and says the scan did not finish. Nested repositories the scan
    /// did not enter are counted, and the lead reaches into each of them (a
    /// row with no name to search names them instead); they are no evidence
    /// on their own, so they never make the row partial.
    pub(super) fn disclose_ignored(&self, row: &mut Value, query: &LspSearchQuery, scope: &Scope) {
        let ignored = &self.ignored;
        if !ignored.nested_repositories.is_empty() {
            // The `includeSkipped` lead lists each one; without a name to
            // search there is no lead, so the notice names them.
            let listed = if lsp_search_query::symbol_name(query).is_some() {
                String::new()
            } else {
                format!(": {}", ignored.nested_repositories.join(", "))
            };
            crate::response::paging::push_notice(
                row,
                format!(
                    "The scan for gitignored files did not enter {} (directories with their own .git) under the scope root{listed}.",
                    match ignored.nested_repositories.len() {
                        1 => "1 nested repository".to_owned(),
                        count => format!("{count} nested repositories"),
                    },
                ),
            );
        }
        let Some(name) = lsp_search_query::symbol_name(query).filter(|_| {
            !ignored.files.is_empty()
                || ignored.unfinished
                || !ignored.nested_repositories.is_empty()
        }) else {
            return;
        };
        let evidence = ignored
            .importing
            .iter()
            .chain(&ignored.unchecked)
            .cloned()
            .collect::<Vec<_>>();
        let spelled = if evidence.is_empty() {
            &ignored.files
        } else {
            &evidence
        };
        let nested = ignored.nested_repositories.iter().map(|repo| {
            Path::new(&scope.root)
                .join(repo)
                .to_string_lossy()
                .into_owned()
        });
        let listed = spelled.iter().cloned().chain(nested).collect::<Vec<_>>();
        row["next"]["includeSkipped"] = ignored_text_lead(name, scope, &listed, ignored.unfinished);
        if evidence.is_empty() {
            if ignored.unfinished {
                crate::response::paging::mark_partial(row, IGNORED_REASON, &[]);
                crate::response::paging::push_notice(row, IGNORED_UNFINISHED_NOTICE);
            }
            return;
        }
        let rules = match ignored.rules.as_slice() {
            [] => "ignore rules".to_owned(),
            rules => rules.join(", "),
        };
        let mut counts = Vec::new();
        match ignored.importing.len() {
            0 => {}
            1 => counts.push("1 imports it".to_owned()),
            count => counts.push(format!("{count} import it")),
        }
        if !ignored.unchecked.is_empty() {
            counts.push(format!(
                "{} could not be checked for an import (unreadable, larger than {} MiB, unparsable, or past the first {CODE_CHECK_MAX_FILES} checked)",
                ignored.unchecked.len(),
                CODE_CHECK_MAX_BYTES / (1024 * 1024),
            ));
        }
        let more = if ignored.unfinished {
            " The scan for gitignored files stopped at its time bound, so more may exist."
        } else {
            ""
        };
        let warning = format!(
            "Of the gitignored files that spell this name ({rules}), {}; importer recovery did not verify them, and a localSearch with includeIgnored reads their text.{more}",
            counts.join(" and ")
        );
        crate::response::paging::mark_partial(row, IGNORED_REASON, &[warning]);
    }

    /// Record the scan on the row's coverage so readers and
    /// `inferred_project::annotate` know importers were checked. A window
    /// with later windows left is partial (not a terminal limit) and, on
    /// its last location page, carries `next.nextImporterPage`.
    pub(in super::super) fn annotate(
        &self,
        row: &mut Value,
        query: &LspSearchQuery,
        scope: &Scope,
    ) {
        if row.get("status").and_then(Value::as_str) == Some("error") {
            return;
        }
        let files = self.per_file().count();
        let more = self.window.as_ref().is_some_and(Window::more);
        if let Some(payload) = row.as_object_mut() {
            let coverage = payload.entry("coverage").or_insert_with(|| json!({}));
            coverage["importerScan"] = json!(if self.failed {
                SCAN_FAILED
            } else if more {
                SCAN_CAPPED
            } else {
                SCAN_COMPLETE
            });
            coverage["verifiedImporterFiles"] = json!(files);
        }
        if let Some(window) = &self.window {
            self.settle_silent_page(row, query, window);
        }
        self.disclose_ignored(row, query, scope);
        let Some(window) = self.window.as_ref().filter(|window| window.more()) else {
            return;
        };
        let mut base = lsp_search_query::to_row(query);
        if let Some(fields) = base.as_object_mut() {
            fields.remove("page");
        }
        let Some(next_importers) = crate::response::paging::next_page_call(
            row,
            crate::id::ToolId::LspSearch,
            None,
            base,
            "importerPage",
            usize::try_from(window.page).map_or(usize::MAX, |page| page.saturating_add(1)),
            &[("snapshot", json!(window.digest))],
        ) else {
            return;
        };
        let range = window.range(window.page + 1);
        let last_location_page =
            row.pointer("/pagination/hasMore").and_then(Value::as_bool) != Some(true);
        if !self.failed {
            let (from, to) = window.verified_candidates();
            let warning = format!(
                "{total} candidate files mention this name outside the server's answer; importer recovery verifies {MAX_CANDIDATE_FILES} at a time. Verified candidates {from}-{to} of {total}; the importer lead (offered on the last page of sites) verifies candidates {}-{}.",
                range.start + 1,
                range.end,
                total = window.total,
            );
            flag_partial(row, query, CAPPED_REASON, Some(&warning), scope);
        }
        if !last_location_page {
            return;
        }
        row["next"]["nextImporterPage"] = next_importers;
    }
}
