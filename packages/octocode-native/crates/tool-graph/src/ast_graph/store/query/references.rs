//! Exact reference populations shared by identity resolution and candidate pages.
use super::*;
use octocode_runtime_base::execution::cancel::CancellationCheck;
use std::ops::{ControlFlow, Range};

enum Match {
    Key(u32),
    Name(String),
    SymbolSuffix(String),
    FileSuffix(String),
}

pub(super) struct Candidates<'g> {
    g: &'g Graph,
    normalized: String,
    kind: Option<NodeKind>,
    matching: Match,
}

impl Candidates<'_> {
    fn scan(
        &self,
        cancel: &dyn CancellationCheck,
        mut emit: impl FnMut(u32) -> ControlFlow<()>,
    ) -> Result<(), GraphOutput> {
        let ids: Box<dyn Iterator<Item = u32> + '_> = match &self.matching {
            Match::Key(id) => Box::new(std::iter::once(*id)),
            Match::Name(lower) => Box::new(self.g.name_range(lower)),
            _ => Box::new(self.g.t.node_ids()),
        };
        for id in ids {
            check(cancel)?;
            let node = &self.g.t.nodes[id as usize];
            if self.kind.is_some_and(|kind| node.kind != kind) {
                continue;
            }
            let admits = match &self.matching {
                Match::Key(_) => true,
                Match::Name(_) => self.g.t.str(node.name) == self.normalized,
                Match::SymbolSuffix(suffix) => self.g.key(id).ends_with(suffix),
                Match::FileSuffix(suffix) => {
                    node.kind == NodeKind::File && self.g.key(id).ends_with(suffix)
                }
            };
            if admits && emit(id).is_break() {
                break;
            }
        }
        Ok(())
    }

    fn any(&self, cancel: &dyn CancellationCheck) -> Result<bool, GraphOutput> {
        let mut any = false;
        self.scan(cancel, |_| {
            any = true;
            ControlFlow::Break(())
        })?;
        Ok(any)
    }

    fn indices(
        &self,
        range: Range<usize>,
        cancel: &dyn CancellationCheck,
    ) -> Result<Page<u32>, GraphOutput> {
        let mut window = Window::new(range);
        let mut failed = None;
        self.scan(cancel, |id| match window.try_push(|| Ok(id)) {
            Ok(()) => ControlFlow::Continue(()),
            Err(error) => {
                failed = Some(error);
                ControlFlow::Break(())
            }
        })?;
        if let Some(error) = failed {
            return Err(error);
        }
        Ok(window.finish())
    }

    fn project(
        &self,
        indices: Page<u32>,
        cancel: &dyn CancellationCheck,
    ) -> Result<Page, GraphOutput> {
        let rows = indices
            .rows
            .into_iter()
            .map(|id| self.g.node_json(id, cancel).map(Value::Object))
            .collect::<Result<_, _>>()?;
        Ok(Page {
            rows,
            total: indices.total,
            start: indices.start,
        })
    }

    pub(super) fn page(
        &self,
        range: Range<usize>,
        cancel: &dyn CancellationCheck,
    ) -> Result<Page, GraphOutput> {
        self.project(self.indices(range, cancel)?, cancel)
    }
}

impl Graph {
    pub(super) fn reference_candidates<'g>(
        &'g self,
        reference: &str,
        kind: Option<NodeKind>,
        live_alias: bool,
        cancel: &dyn CancellationCheck,
    ) -> Result<Candidates<'g>, GraphOutput> {
        let normalized = normalize(reference);
        let mut candidates = Candidates {
            g: self,
            kind,
            matching: Match::Name(normalized.to_lowercase()),
            normalized,
        };
        let exact = self
            .reference_keys(&candidates.normalized, live_alias)
            .into_iter()
            .find_map(|key| {
                self.t
                    .by_key(&key)
                    .filter(|id| kind.is_none_or(|kind| self.t.nodes[*id as usize].kind == kind))
            });
        if let Some(id) = exact {
            candidates.matching = Match::Key(id);
            return Ok(candidates);
        }
        if candidates.any(cancel)? {
            return Ok(candidates);
        }
        if candidates.normalized.contains(['.', '#']) {
            candidates.matching = Match::SymbolSuffix(format!(
                "#{}",
                candidates.normalized.trim_start_matches('#')
            ));
            if candidates.any(cancel)? {
                return Ok(candidates);
            }
        }
        candidates.matching = Match::FileSuffix(format!("/{}", candidates.normalized));
        Ok(candidates)
    }

    /// The snapshot keys a normalized reference may spell: as written, and
    /// root-relative when it is a path under the snapshot root. Live aliases
    /// are only an initial exact-key convenience: continuations enumerate the
    /// immutable snapshot using the normalized reference.
    pub(super) fn reference_keys(&self, normalized: &str, live_alias: bool) -> Vec<String> {
        let root = Path::new(self.manifest["root"].as_str().unwrap_or_default());
        let path = Path::new(normalized);
        let mut spellings = vec![path.to_path_buf()];
        if live_alias {
            // A relative path also reads from the workspace; each spelling
            // also with its deepest existing ancestor canonical, as the
            // root is (`/var` is `/private/var` on macOS).
            if path.is_relative()
                && let Some(workspace) = self.manifest["workspace"].as_str()
            {
                spellings.push(Path::new(workspace).join(path));
            }
            let canonical = spellings.iter().filter_map(|spelling| canonical(spelling));
            spellings = spellings.iter().cloned().chain(canonical).collect();
        }
        let mut keys = vec![normalized.to_owned()];
        // A path from the workspace, as answers name files, is the key under
        // the scope it names: lexical, so a continuation reads it the same.
        let shown = self.shown_root();
        if shown != "."
            && let Some(key) = normalized
                .strip_prefix(shown.as_str())
                .and_then(|rest| rest.strip_prefix('/'))
                .filter(|key| !key.is_empty())
        {
            keys.push(key.to_owned());
        }
        for spelling in spellings {
            if let Ok(relative) = spelling.strip_prefix(root) {
                let relative = match octocode_engine::text::slash_path(relative) {
                    relative if relative.is_empty() => ".".to_owned(),
                    relative => relative,
                };
                if !keys.contains(&relative) {
                    keys.push(relative);
                }
            }
        }
        keys
    }

    /// The directory `reference` names when it names no node: its
    /// root-relative path (`.` for the root) and every file under it, in key
    /// order. `None` when it spells a node key or no file lies under it.
    pub(super) fn directory(&self, reference: &str) -> Option<(String, Vec<u32>)> {
        let normalized = normalize(reference);
        if names_nothing(&normalized) {
            return None;
        }
        let keys = self.reference_keys(&normalized, true);
        if keys.iter().any(|key| self.t.by_key(key).is_some()) {
            return None;
        }
        keys.iter().find_map(|key| {
            // Only `.` is the root: `/` and an empty reference name none.
            let dir = if key == "." {
                ""
            } else {
                key.trim_end_matches('/')
            };
            if dir.is_empty() && key != "." {
                return None;
            }
            let files = self.files_under(dir);
            (!files.is_empty()).then(|| {
                let shown = if dir.is_empty() { "." } else { dir };
                (shown.to_owned(), files)
            })
        })
    }

    /// Every file under root-relative directory `dir` (`""` or `.`: the
    /// root), in key order.
    fn files_under(&self, dir: &str) -> Vec<u32> {
        let prefix = format!("{dir}/");
        let whole = dir.is_empty() || dir == ".";
        let mut files = self
            .t
            .node_ids()
            .filter(|id| {
                self.t.nodes[*id as usize].kind == NodeKind::File
                    && (whole || self.key(*id).starts_with(&prefix))
            })
            .collect::<Vec<_>>();
        files.sort_by(|a, b| self.key(*a).cmp(self.key(*b)));
        files
    }

    /// The key prefix (`dir/`) of the deepest directory along a path-shaped
    /// reference that holds snapshot files: the place a missed path's
    /// suggestions must come from. `None` for a bare name, or when only the
    /// root does (every key lies under it).
    fn suggestion_scope(&self, normalized: &str) -> (String, Option<String>) {
        let text = self
            .reference_keys(normalized, true)
            .into_iter()
            .find(|key| !Path::new(key).is_absolute())
            .unwrap_or_else(|| normalized.to_owned());
        let mut dir = text.trim_end_matches('/');
        let prefix = loop {
            let Some((parent, _)) = dir.rsplit_once('/') else {
                break None;
            };
            dir = parent;
            if dir.is_empty() {
                break None;
            }
            if !self.files_under(dir).is_empty() {
                break Some(format!("{dir}/"));
            }
        };
        (text, prefix)
    }
}

/// A graph reference no node matches: the snapshot, not a registry, is
/// where it was looked up.
const NOT_FOUND_HINT: &str = "No node in this snapshot has that key or name: run the find lead, or rebuild with graphBuild when the file is newer than the snapshot.";

impl Query<'_> {
    /// The name a reference's last segment holds (after its last `#`, else
    /// its file name without extension), and whether it names a file: its
    /// extension is one of the snapshot's file extensions.
    fn reference_name(&self, text: &str) -> (String, bool) {
        if let Some((_, name)) = text.rsplit_once('#') {
            return (name.to_owned(), false);
        }
        let base = text.rsplit('/').next().unwrap_or(text);
        if let Some((stem, extension)) = base.rsplit_once('.')
            && !stem.is_empty()
            && !extension.is_empty()
        {
            let suffix = format!(".{extension}");
            let g = self.g;
            if g.t
                .nodes
                .iter()
                .any(|node| node.kind == NodeKind::File && g.t.str(node.key).ends_with(&suffix))
            {
                return (stem.to_owned(), true);
            }
        }
        (base.to_owned(), false)
    }
}

/// A reference as written, without surrounding space or a leading `./`.
fn normalize(reference: &str) -> String {
    reference.trim().trim_start_matches("./").to_owned()
}

/// A normalized reference that is empty or only slashes (`./`, blanks, `/`)
/// names no node and no directory; `.` names the root.
fn names_nothing(normalized: &str) -> bool {
    normalized.trim_matches('/').is_empty()
}

/// `path` with its deepest existing ancestor canonical, so a live path that
/// names a missing file still compares with the canonical root.
fn canonical(path: &Path) -> Option<PathBuf> {
    let mut rest = Vec::new();
    let mut base = path;
    loop {
        if let Ok(real) = std::fs::canonicalize(base) {
            return Some(rest.iter().rev().fold(real, |path, part| path.join(part)));
        }
        rest.push(base.file_name()?);
        base = base.parent()?;
    }
}

impl Query<'_> {
    pub(super) fn candidates(&self) -> Result<GraphOutput, GraphOutput> {
        let reference = self.options.target.as_deref().ok_or_else(|| {
            GraphOutput::error(
                ErrorCode::InvalidInput,
                "`candidates` needs a node reference",
            )
        })?;
        let matches =
            self.g
                .reference_candidates(reference, self.flags.kind, false, self.cancel)?;
        let page = matches.page(self.window(), self.cancel)?;
        Ok(run_page(self.options, self.g, Map::new(), page))
    }

    pub(super) fn resolve(
        &self,
        reference: &str,
        kind: Option<NodeKind>,
    ) -> Result<u32, GraphOutput> {
        let not_found = || {
            let mut out = GraphOutput::error(
                ErrorCode::NotFound,
                format!("no node matches {reference:?}; locate it with graphQuery operation find"),
            );
            octocode_runtime_base::response::result::add_hints(&mut out.value, [NOT_FOUND_HINT]);
            out
        };
        // Nothing to match, so nothing to suggest.
        if names_nothing(&normalize(reference)) {
            return Err(not_found());
        }
        let matches = self
            .g
            .reference_candidates(reference, kind, true, self.cancel)?;
        let indices = matches.indices(0..MAX_CANDIDATES, self.cancel)?;
        if indices.total == 1 {
            return Ok(indices.rows[0]);
        }
        if indices.total == 0 {
            let mut out = not_found();
            // A missed path suggests only nodes under its deepest directory
            // the snapshot holds, never same-word nodes elsewhere.
            let (text, prefix) = self.g.suggestion_scope(&matches.normalized);
            // A path suggests by its name, not by the directory words every
            // node under it shares; a file path suggests only files.
            let (name, file_path) = self.reference_name(&text);
            let kind = kind.or(file_path.then_some(NodeKind::File));
            // Only near names stand in for the reference: a node sharing
            // one word of a longer name is the find lead's to list.
            let suggestions = ranked(self.g, &name, kind, self.cancel)?
                .into_iter()
                .filter(|hit| is_near(self.g, &name, hit))
                .map(|hit| self.g.key(hit.id).to_owned())
                .filter(|key| prefix.as_ref().is_none_or(|prefix| key.starts_with(prefix)))
                .take(MAX_SUGGESTIONS)
                .collect::<Vec<_>>();
            if !suggestions.is_empty() {
                out.value["suggestions"] = json!(suggestions);
            }
            if !name.is_empty() {
                out.value["find"] = json!(name);
            }
            return Err(out);
        }
        let page = matches.project(indices, self.cancel)?;
        let mut out = GraphOutput::error(
            ErrorCode::InvalidInput,
            format!(
                "{} nodes match {reference:?}; pass one of the candidate ids",
                page.total
            ),
        );
        out.value["graph"] = self.g.summary();
        let shown = page.rows.len();
        // Each listed candidate's call: this request with the reference
        // replaced by that candidate's exact id, runnable as written.
        let calls = page
            .rows
            .iter()
            .filter_map(|candidate| candidate["id"].as_str())
            .map(|id| {
                let mut row = query_row(self.options, &self.g.id);
                naming(&mut row, reference, &self.g.shown_key(id));
                octocode_runtime_base::response::result::Continuation::related(
                    octocode_runtime_base::id::ToolId::GraphQuery,
                    Value::Object(row),
                )
                .build()
            })
            .collect::<Vec<_>>();
        if !calls.is_empty() {
            out.value["related"] = Value::Array(calls);
        }
        out.value["candidates"] = json!(page.rows);
        out.value["pagination"] = pagination(
            self.options.offset.unwrap_or(0),
            shown,
            page.total,
            shown < page.total,
        );
        if shown < page.total {
            let next = QueryOptions {
                op: "candidates".into(),
                path: self.options.path.clone(),
                include_provenance: self.options.include_provenance,
                target: Some(matches.normalized),
                kind: kind.map(|kind| kind.as_str().to_owned()),
                workspace: self.options.workspace.clone(),
                home: self.options.home.clone(),
                graph: self.options.graph.clone(),
                offset: Some(shown),
                ..Default::default()
            };
            out.next
                .push((Channel::NextPage, Resume::Query(Box::new(next))));
        }
        Err(out)
    }
}

/// `row` with each field that names `reference` (a target, `to`, a
/// `changed` or `entries` item) naming `id` instead.
fn naming(row: &mut Map<String, Value>, reference: &str, id: &str) {
    for field in ["target", "to", "changed", "entries"] {
        match row.get_mut(field) {
            Some(Value::String(named)) if named == reference => *named = id.to_owned(),
            Some(Value::Array(items)) => {
                for item in items {
                    if item == reference {
                        *item = json!(id);
                    }
                }
            }
            _ => {}
        }
    }
}
