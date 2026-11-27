//! `Importers`: per-file anchors, owned files, coverage digests, settling a
//! silent page, and disclosing capped scans.
use super::*;
use octocode_contract_types::LspSearchChannel;
use octocode_runtime_base::response::result::{Continuation, insert_next};

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

    /// Whether a location in `file` (canonical) belongs on this page of an
    /// index-backed answer, which holds every window opened so far: a candidate
    /// on its own window's page, any other file on the first page.
    pub(in super::super) fn lists_on_this_page(&self, file: &str) -> bool {
        self.window.as_ref().is_none_or(|window| {
            if window.lists(file) {
                window.verified_owner(file)
            } else {
                window.first <= 1
            }
        })
    }

    /// Whether the name at `site` binds another declaration: a verified
    /// importer's answer that lists it does not make it a reference.
    pub(in super::super) fn binds_elsewhere(&self, site: &Site) -> bool {
        self.elsewhere.contains(site)
    }

    /// Whether `file` (canonical) is a declaration the anchor package's
    /// build emitted: its locations mirror the anchor's own sources, which
    /// the anchor's answer lists.
    pub(in super::super) fn mirrors_source(&self, file: &str) -> bool {
        self.output
            .as_ref()
            .is_some_and(|output| output.holds(Path::new(file)))
    }

    /// The candidate digest, salted into this request's location snapshot so
    /// a changed candidate list stales the window's later location pages.
    pub(in super::super) fn digest(&self) -> Option<&str> {
        self.window.as_ref().map(Window::digest)
    }

    /// Candidates checked by this window and every earlier one, less this
    /// window's unverified and too-large candidates: those stay text-only so
    /// the row's coverage and lexical lead disclose them.
    pub(in super::super) fn covered(&self) -> Vec<String> {
        let unchecked = self
            .unverified
            .iter()
            .chain(&self.too_large)
            .chain(&self.package)
            .collect::<HashSet<_>>();
        self.window
            .as_ref()
            .map_or(&[][..], Window::covered)
            .iter()
            .filter(|file| !unchecked.contains(file))
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
        object.insert(
            "operation".into(),
            json!(lsp_search_query::operation(query)),
        );
        object.insert("matches".into(), json!([]));
        object.insert(
            "reason".into(),
            json!(format!(
                "Candidates {from}-{to} of {} {held}; {tail}.",
                window.total()
            )),
        );
    }

    /// What importer recovery could not do, naming the step and, for
    /// candidates, every file of the window with its cause (paths relative
    /// to `root`).
    pub(super) fn failure_warning(&self, root: &str) -> String {
        const CONFIRM: &str = "confirm with a localSearch of the name.";
        match self.failure {
            ScanFailure::CandidateList => format!(
                "Importer recovery could not list the files that spell this name: the workspace text scan failed, so no importer was verified; {CONFIRM}"
            ),
            ScanFailure::AnchorDeclaration => format!(
                "Importer recovery could not resolve the anchor's own declaration (the server's definition at the anchor named none), so none of the {} candidate files that spell this name could be checked against it; {CONFIRM}",
                self.window.as_ref().map_or(0, Window::total)
            ),
            ScanFailure::Candidates => {
                let relative = |file: &String| {
                    Path::new(file)
                        .strip_prefix(root)
                        .map_or_else(|_| file.clone(), |path| path.to_string_lossy().into_owned())
                };
                let unopened = self.unopened.iter().collect::<HashSet<_>>();
                let (closed, unresolved): (Vec<_>, Vec<_>) = self
                    .unverified
                    .iter()
                    .partition(|file| unopened.contains(file));
                let mut causes = Vec::new();
                if !closed.is_empty() {
                    causes.push(format!(
                        "{} could not be read or opened in the server",
                        closed
                            .into_iter()
                            .map(relative)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                if !unresolved.is_empty() {
                    causes.push(format!(
                        "the server resolved no occurrence in {} to the anchor's declaration",
                        unresolved
                            .into_iter()
                            .map(relative)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                format!(
                    "Importer recovery could not verify {} candidate file(s): {}. They stay out of the answer; {CONFIRM}",
                    self.unverified.len(),
                    causes.join("; ")
                )
            }
        }
    }

    /// Candidates too large to send the server are outside what it can
    /// verify, which is no failure of the scan: the row counts them
    /// (`coverage.tooLargeFiles`) and, unless a text search already leads
    /// there, leads one to them. They stay text-only, so a lead a partial
    /// answer builds reads them as well.
    fn disclose_too_large(&self, row: &mut Value, query: &LspSearchQuery, scope: &Scope) {
        if self.too_large.is_empty() {
            return;
        }
        if let Some(fields) = row.as_object_mut() {
            fields.entry("coverage").or_insert_with(|| json!({}))["tooLargeFiles"] =
                json!(self.too_large.len());
        }
        let Some(name) = lsp_search_query::symbol_name(query) else {
            return;
        };
        let leads_text = row
            .get("related")
            .and_then(Value::as_array)
            .is_some_and(|related| {
                related.iter().any(|call| {
                    call["tool"] == octocode_runtime_base::id::ToolId::LocalSearch.as_str()
                })
            });
        if !leads_text
            && let Some(lead) = crate::lsp_search::failure::text_search_lead(
                name,
                &scope.root,
                &scope.include,
                Some(&self.too_large),
            )
        {
            octocode_runtime_base::response::result::attach_related(row, lead.build());
        }
    }

    /// Record the scan on the row's coverage so readers and
    /// `inferred_project::annotate` know importers were checked. A window
    /// with later windows left is partial (not a terminal limit), and every
    /// location page of it carries `next.nextImporterWindow`: the same query
    /// on each page, so the same content-addressed cursor, which a reader
    /// that follows every lead verifies once.
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
            // The request's workspaceRoot bounded the candidate scan below
            // the anchor's repository: importers outside it went unscanned.
            if scope.narrowed {
                coverage["importerScanRoot"] = json!("workspaceRoot");
            }
            if !self.package.is_empty() {
                coverage["packageImporters"] = json!(self.package.len());
            }
        }
        if let Some(window) = &self.window {
            self.settle_silent_page(row, query, window);
        }
        if self.failed {
            let warning = self.failure_warning(&scope.root);
            flag_partial(
                row,
                query,
                crate::lsp_search::inferred_project::FAILED_REASON,
                Some(&warning),
                scope,
            );
        }
        // Files bound to the package's own build output, which no source
        // declaration mirrors, are importers the scan could not verify: what
        // `importUnresolved` means is core's; the lead reads them.
        if !self.package.is_empty() {
            flag_partial(
                row,
                query,
                crate::lsp_search::server_coverage::UNRESOLVED_IMPORT_REASON,
                None,
                scope,
            );
        }
        self.disclose_too_large(row, query, scope);
        let Some(window) = self.window.as_ref().filter(|window| window.more()) else {
            return;
        };
        let Ok(next_importers) = lsp_search_query::edited(
            query,
            &json!({
                "page": null,
                "importerPage": window.page.saturating_add(1),
                "snapshot": window.digest(),
            }),
        ) else {
            return;
        };
        let range = window.range(window.page + 1);
        if !self.failed {
            let (from, to) = window.verified_candidates();
            let warning = format!(
                "{total} candidate files mention this name outside the server's answer; importer recovery verifies {MAX_CANDIDATE_FILES} at a time. Verified candidates {from}-{to} of {total}; the importer lead verifies candidates {}-{}.",
                range.start + 1,
                range.end,
                total = window.total(),
            );
            flag_partial(row, query, CAPPED_REASON, Some(&warning), scope);
        }
        insert_next(
            &mut row["next"],
            LspSearchChannel::NextImporterWindow,
            Continuation::continued_query(next_importers),
        );
    }
}
