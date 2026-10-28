//! Exact reference populations shared by identity resolution and candidate pages.
use super::*;
use crate::execution::cancel::CancellationCheck;
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
            Match::Name(lower) => Box::new(self.g.name_range(lower, false)),
            _ => Box::new(0..self.g.t.nodes.len() as u32),
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
        let normalized = reference.trim().trim_start_matches("./").to_owned();
        let mut candidates = Candidates {
            g: self,
            kind,
            matching: Match::Name(normalized.to_lowercase()),
            normalized,
        };
        let root = self.manifest["root"].as_str().unwrap_or_default();
        // Live aliases are only an initial exact-key convenience. Continuations
        // enumerate the immutable snapshot using the normalized failed reference.
        let absolute = if live_alias {
            std::fs::canonicalize(&candidates.normalized)
                .unwrap_or_else(|_| PathBuf::from(&candidates.normalized))
        } else {
            PathBuf::from(&candidates.normalized)
        };
        let relative = absolute
            .strip_prefix(root)
            .ok()
            .map(octocode_engine::text::slash_path);
        let exact = [Some(candidates.normalized.as_str()), relative.as_deref()]
            .into_iter()
            .flatten()
            .find_map(|key| {
                self.t
                    .by_key(key)
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
        let matches = self
            .g
            .reference_candidates(reference, kind, true, self.cancel)?;
        let indices = matches.indices(0..MAX_CANDIDATES, self.cancel)?;
        if indices.total == 1 {
            return Ok(indices.rows[0]);
        }
        if indices.total == 0 {
            let mut out = GraphOutput::error(
                ErrorCode::NotFound,
                format!("no node matches {reference:?}; try `octocode graph query find <text>`"),
            );
            let mut suggestions = Vec::new();
            let lower = matches.normalized.to_lowercase();
            for id in self.g.name_range(&lower, true) {
                check(self.cancel)?;
                if kind.is_none_or(|kind| self.g.t.nodes[id as usize].kind == kind) {
                    suggestions.push(self.g.key(id).to_owned());
                    if suggestions.len() == 5 {
                        break;
                    }
                }
            }
            if !suggestions.is_empty() {
                out.value["suggestions"] = json!(suggestions);
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
        out.value["total"] = json!(page.total);
        let shown = page.rows.len();
        out.value["candidates"] = json!(page.rows);
        if shown < page.total {
            let next = QueryOptions {
                op: "candidates".into(),
                target: Some(matches.normalized),
                kind: kind.map(|kind| kind.as_str().to_owned()),
                workspace: self.options.workspace.clone(),
                graph: self.options.graph.clone(),
                offset: Some(shown),
                ..Default::default()
            };
            out.value["truncated"] = json!(true);
            out.value["next"] = json!(next_command(&next, &self.g.id, shown));
            out.next_query = Some(Box::new(next));
        }
        Err(out)
    }
}
