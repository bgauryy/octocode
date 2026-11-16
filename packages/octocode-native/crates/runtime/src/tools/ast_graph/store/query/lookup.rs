//! Graph lookups: node keys and JSON, reference resolution, neighbors and
//! re-export users; plus the edge filter a walk follows.
use super::*;

/// `path` (a snapshot key, `..` for a config file above the scope) named from
/// the directory `root` names, lexically: `app` and `../package.json` are
/// `package.json`. An absolute `path` names itself.
fn join(root: &str, path: &str) -> String {
    if path.starts_with('/') {
        return path.to_owned();
    }
    let mut parts: Vec<&str> = root
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|last| *last != "..") => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    let joined = parts.join("/");
    match (root.starts_with('/'), joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}

/// Answer fields whose values name nodes, directories or findings (a key,
/// a list of keys, or rows): the ones that name a snapshot file or directory
/// are restated from the workspace ([`Graph::rebase_paths`]).
const REFERENCE_FIELDS: [&str; 13] = [
    "id",
    "subject",
    "from",
    "to",
    "target",
    "members",
    "dirs",
    "nodes",
    "rows",
    "files",
    "reachedFrom",
    "affectedEntrypoints",
    "testsToRun",
];

/// Answer fields whose values are `path:line` mentions (a list, or lists
/// keyed by export name) or text file paths the mention scan could not read
/// (`unread`): each path is a file's, restated from the workspace whether or
/// not the snapshot holds it (a README, a script).
const MENTION_FIELDS: [&str; 3] = ["mentionedIn", "textMentions", "unread"];

/// Each `path:line` mention under `value` with its path restated from the
/// workspace.
fn rebase_mentions(value: &mut Value, root: &str) {
    match value {
        Value::String(mention) => {
            let (path, line) = mention
                .rsplit_once(':')
                .filter(|(_, line)| !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit()))
                .map_or((mention.as_str(), None), |(path, line)| (path, Some(line)));
            let path = join(root, path);
            *mention = match line {
                Some(line) => format!("{path}:{line}"),
                None => path,
            };
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| rebase_mentions(item, root)),
        Value::Object(fields) => fields
            .values_mut()
            .for_each(|item| rebase_mentions(item, root)),
        _ => {}
    }
}

impl Graph {
    pub(super) fn node_record(
        &self,
        id: u32,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<super::super::format::NodeRec, GraphOutput> {
        if let Some(reader) = &self.selective
            && reader.borrow().is_point()
        {
            reader
                .borrow_mut()
                .node(id, &self.t, cancel)
                .map_err(|message| *decode_failure(message, &self.manifest, cancel))
        } else {
            Ok(self.t.nodes[id as usize])
        }
    }

    fn declaration(
        &self,
        id: u32,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<Option<super::super::format::DeclarationRec>, GraphOutput> {
        if let Some(reader) = &self.selective {
            reader
                .borrow_mut()
                .declaration(id, &self.t, cancel)
                .map_err(|message| *decode_failure(message, &self.manifest, cancel))
        } else {
            Ok(self
                .t
                .declarations
                .binary_search_by_key(&id, |d| d.node)
                .ok()
                .map(|index| self.t.declarations[index]))
        }
    }

    pub(super) fn key(&self, id: u32) -> &str {
        self.t.str(self.t.nodes[id as usize].key)
    }

    /// The file a node lies in: a symbol's file; a file or package itself.
    pub(super) fn file_of(&self, id: u32) -> u32 {
        let node = &self.t.nodes[id as usize];
        match node.kind {
            NodeKind::Symbol => node.file,
            NodeKind::File | NodeKind::Package => id,
        }
    }

    /// The module a node belongs to: the root-relative directory of its file
    /// (`.` for the root); a package is its own module.
    pub(super) fn module(&self, id: u32) -> &str {
        if self.t.nodes[id as usize].kind == NodeKind::Package {
            return self.key(id);
        }
        self.key(self.file_of(id))
            .rsplit_once('/')
            .map_or(".", |(dir, _)| dir)
    }

    pub(super) fn node_json(
        &self,
        id: u32,
        cancel: &dyn crate::execution::cancel::CancellationCheck,
    ) -> Result<Map<String, Value>, GraphOutput> {
        let node = self.node_record(id, cancel)?;
        let mut out = Map::new();
        out.insert("id".into(), json!(self.key(id)));
        out.insert("kind".into(), json!(node.kind.as_str()));
        let detail = self.t.str(node.detail);
        match node.kind {
            NodeKind::File => {
                if !detail.is_empty() {
                    out.insert("language".into(), json!(detail));
                }
                let roles = super::super::super::classify::role_names(node.flags);
                if !roles.is_empty() {
                    out.insert("roles".into(), json!(roles));
                }
            }
            NodeKind::Symbol => {
                // A point answer's keys come from lookup entries: the file's
                // is checked against its node record before it is rendered.
                let file_record = self.node_record(node.file, cancel)?;
                if let Some(decl) = self.declaration(id, cancel)? {
                    let file = self.key(node.file);
                    let digest = self.t.str(decl.source_digest);
                    if let Ok(identity) =
                        serde_json::from_str::<Value>(self.t.str(decl.test_identity))
                        && !identity.is_null()
                    {
                        out.insert("testIdentity".into(), identity);
                    }
                    // A file a read hides any of states no byte provenance
                    // ([`Graph::shows_provenance`]): its line places the row.
                    let shown = Self::shows_provenance(&file_record);
                    if shown {
                        out.insert("occurrenceId".into(), json!(self.t.str(decl.occurrence)));
                        out.insert("sourceDigest".into(), json!(digest));
                    }
                    if shown && decl.source_start != NONE {
                        out.insert(
                            "sourceRange".into(),
                            sources::range(octocode_engine::text::analysis::ByteRange::new(
                                decl.source_start,
                                decl.source_end,
                            ))?,
                        );
                        out.insert(
                            "sourceOrigin".into(),
                            self.project_origin(decl.source_origin, file, digest, cancel)?
                                .value,
                        );
                    }
                    if shown && decl.selection_start != NONE {
                        out.insert(
                            "selectionByteRange".into(),
                            sources::range(octocode_engine::text::analysis::ByteRange::new(
                                decl.selection_start,
                                decl.selection_end,
                            ))?,
                        );
                        out.insert(
                            "selectionOrigin".into(),
                            self.project_origin(decl.selection_origin, file, digest, cancel)?
                                .value,
                        );
                    }
                    if let Ok(assessment) = serde_json::from_str(self.t.str(decl.assessment)) {
                        out.insert("parseAssessment".into(), assessment);
                    }
                }
                out.insert("symbolName".into(), json!(self.t.str(node.name)));
                // A symbol's kind is its declaration's (function, class, …).
                out.insert("kind".into(), json!(detail));
                out.insert("path".into(), json!(self.key(node.file)));
                out.insert("line".into(), json!(node.line));
                if node.end_line != node.line {
                    out.insert("endLine".into(), json!(node.end_line));
                }
                if node.flags & FLAG_EXPORTED != 0 {
                    out.insert("exported".into(), json!(true));
                }
                if node.flags & super::super::format::FLAG_TEST != 0 {
                    out.insert("test".into(), json!(true));
                }
            }
            NodeKind::Package => {
                out.insert("ecosystem".into(), json!(detail));
            }
        }
        Ok(out)
    }

    pub(super) fn edge_fields(&self, out: &mut Map<String, Value>, edge: &EdgeRec) {
        out.insert("edge".into(), json!(edge.kind.as_str()));
        let via = self.t.str(edge.detail);
        if !via.is_empty() && edge.kind != EdgeKind::Contains {
            out.insert("via".into(), json!(via));
        }
        if edge.line != NONE {
            out.insert("edgeLine".into(), json!(edge.line));
        }
        if edge.kind == EdgeKind::Calls {
            out.insert("confidence".into(), json!(edge.confidence.as_str()));
        }
        if edge.test {
            out.insert("test".into(), json!(true));
        }
    }

    /// The scope named relative to the workspace, as graphBuild states it;
    /// absolute when it lies outside the workspace.
    pub(super) fn shown_root(&self) -> String {
        let root = self.manifest["root"].as_str().unwrap_or_default();
        self.manifest["workspace"]
            .as_str()
            .and_then(|workspace| Path::new(root).strip_prefix(workspace).ok())
            .map(|relative| match relative.as_os_str().is_empty() {
                true => ".".to_owned(),
                false => octocode_engine::text::slash_path(relative),
            })
            .unwrap_or_else(|| root.to_owned())
    }

    /// States every file reference of an answer from the workspace root, as
    /// every local tool names a file: the snapshot keys files from its own
    /// root (`graph.root`). Each `path`, and each reference field
    /// ([`REFERENCE_FIELDS`]) that names a snapshot file or directory, is
    /// restated; node ids that are ids (a symbol's `file#name`, a package's
    /// `pkg:` key) and finding ids stay as they are. Calls (`next`,
    /// `related`, `reingest`) carry request paths, which already name the
    /// workspace.
    pub(super) fn rebase_paths(&self, value: &mut Value) {
        let root = self.shown_root();
        if root != "." {
            self.rebase(value, &root);
        }
    }

    fn rebase(&self, value: &mut Value, root: &str) {
        match value {
            Value::Array(items) => items.iter_mut().for_each(|item| self.rebase(item, root)),
            Value::Object(object) => {
                for (key, item) in object.iter_mut() {
                    match (key.as_str(), item) {
                        ("path", Value::String(path)) => *path = join(root, path),
                        ("next" | "related" | "reingest" | "graph", _) => {}
                        (key, item) if REFERENCE_FIELDS.contains(&key) => {
                            self.rebase_reference(item, root);
                        }
                        (key, item) if MENTION_FIELDS.contains(&key) => rebase_mentions(item, root),
                        (_, item) => self.rebase(item, root),
                    }
                }
            }
            _ => {}
        }
    }

    /// A reference field's value: a key, a list of keys, or an object (a
    /// node, a hop) whose own fields say what they hold.
    fn rebase_reference(&self, value: &mut Value, root: &str) {
        match value {
            Value::String(key) if self.names_path(key) => *key = join(root, key),
            Value::Array(items) => items
                .iter_mut()
                .for_each(|item| self.rebase_reference(item, root)),
            Value::Object(_) => self.rebase(value, root),
            _ => {}
        }
    }

    /// Whether `key` names a snapshot file, or a directory holding one (a
    /// module; `.` is the root): a path, not an opaque id.
    fn names_path(&self, key: &str) -> bool {
        if key == "." {
            return true;
        }
        if let Some(id) = self.t.by_key(key) {
            return self.t.nodes[id as usize].kind == NodeKind::File;
        }
        let prefix = format!("{}/", key.trim_end_matches('/'));
        let keys = &self.t.key_index;
        let at = keys.partition_point(|id| self.key(*id) < prefix.as_str());
        keys.get(at)
            .is_some_and(|id| self.key(*id).starts_with(&prefix))
    }

    /// The snapshot every answer names. Files the scan skipped as
    /// unreadable or oversized leave every answer partial: `partialReasons`
    /// names the cut. A snapshot bound is no cut: a build past one is
    /// refused, never published.
    pub(super) fn summary(&self) -> Value {
        let mut out = json!({"id": self.id});
        if self.manifest["scan"]["filesSkipped"].as_u64().unwrap_or(0) > 0 {
            crate::response::paging::mark_partial(
                &mut out,
                crate::response::paging::PartialReason::FilesSkipped,
                &[],
            );
        }
        out
    }

    /// Node ids whose lowercase name equals `lower`, via the sorted name index.
    pub(super) fn name_range<'a>(&'a self, lower: &'a str) -> impl Iterator<Item = u32> + 'a {
        let lowered = |id: u32| self.t.str(self.t.nodes[id as usize].name).to_lowercase();
        let start = self
            .t
            .name_index
            .partition_point(|id| lowered(*id).as_str() < lower);
        self.t.name_index[start..]
            .iter()
            .copied()
            .take_while(move |id| lowered(*id) == lower)
    }

    pub(super) fn neighbors<'a>(
        &'a self,
        node: u32,
        direction: Direction,
        filter: &'a EdgeFilter,
    ) -> impl Iterator<Item = (u32, &'a EdgeRec)> + 'a {
        let out = (direction != Direction::In)
            .then(|| self.t.out(node).iter().map(|edge| (edge.dst, edge)))
            .into_iter()
            .flatten();
        let incoming = (direction != Direction::Out)
            .then(|| self.t.incoming(node).map(|edge| (edge.src, edge)))
            .into_iter()
            .flatten();
        out.chain(incoming).filter(|(_, edge)| filter.admits(edge))
    }
}

impl Graph {
    /// The files that use an export of `file` through a re-exporting
    /// module: `uses` edges resolved through a re-export into a symbol
    /// `file` declares, from another file.
    pub(super) fn reexport_users(&self, file: u32) -> impl Iterator<Item = (u32, &EdgeRec)> + '_ {
        contained_symbols(self, file)
            .into_iter()
            .flat_map(move |symbol| {
                self.t
                    .incoming(symbol)
                    .filter(move |edge| {
                        edge.kind == EdgeKind::Uses
                            && self.t.str(edge.detail) == "reexport"
                            && edge.src != file
                            && self.t.nodes[edge.src as usize].kind == NodeKind::File
                    })
                    .map(|edge| (edge.src, edge))
            })
    }
}

pub(super) struct EdgeFilter {
    pub(super) kinds: Vec<EdgeKind>,
    pub(super) weakest: Confidence,
    /// Admit test edges: traversals do; the production rankings (`hubs`,
    /// `cycles`, `stats`) do not.
    pub(super) tests: bool,
}

impl EdgeFilter {
    pub(super) fn admits(&self, edge: &EdgeRec) -> bool {
        self.admits_kind(edge) && !self.below_confidence(edge)
    }

    /// Admits the edge's kind and test role, whatever its confidence.
    pub(super) fn admits_kind(&self, edge: &EdgeRec) -> bool {
        self.kinds.contains(&edge.kind) && (self.tests || !edge.test)
    }

    /// A call edge weaker than the filter's confidence.
    pub(super) fn below_confidence(&self, edge: &EdgeRec) -> bool {
        edge.kind == EdgeKind::Calls && edge.confidence > self.weakest
    }

    /// The same filter over production edges only.
    pub(super) fn production(mut self) -> Self {
        self.tests = false;
        self
    }
}

#[cfg(test)]
mod join_tests {
    /// A snapshot key named from the workspace root: the scope joined in,
    /// a config file above the scope folded in, an outside scope kept whole.
    #[test]
    fn a_key_joins_its_scope_lexically() {
        for (root, key, shown) in [
            (
                "packages/cli",
                "src/json-schema.ts",
                "packages/cli/src/json-schema.ts",
            ),
            ("app", "../package.json", "package.json"),
            ("a/b", "../../../x.json", "../x.json"),
            ("/outside/scope", "src/a.ts", "/outside/scope/src/a.ts"),
            ("app", "/abs/a.ts", "/abs/a.ts"),
        ] {
            assert_eq!(super::join(root, key), shown, "{root} + {key}");
        }
    }
}
