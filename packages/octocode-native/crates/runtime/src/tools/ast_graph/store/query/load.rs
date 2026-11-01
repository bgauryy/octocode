//! Choosing and loading the snapshot a query reads.
use super::*;

pub(super) fn locate(options: &QueryOptions) -> Result<(PathBuf, bool), Failure> {
    if let Some(spec) = options.graph.as_deref() {
        let path = PathBuf::from(spec);
        if path.is_file() {
            return Ok((
                path.parent().map(Path::to_path_buf).unwrap_or_default(),
                true,
            ));
        }
        if path.join(GRAPH_FILE).is_file() {
            return Ok((path, true));
        }
    }
    let home = graph_home(options.workspace.as_deref())
        .map_err(|message| fail(ErrorCode::WorkspaceRootInvalid, message))?;
    let ids = list_snapshots(&home);
    let chosen = match options.graph.as_deref() {
        Some(spec) => choose_snapshot(&home, &ids, spec)?,
        None => std::fs::read_to_string(home.join(LATEST_FILE))
            .ok()
            .map(|id| id.trim().to_owned())
            .filter(|id| ids.contains(id))
            .or_else(|| ids.last().cloned()),
    };
    chosen.map(|id| (home.join(id), false)).ok_or_else(|| {
        let message = match options.graph.as_deref() {
            Some(spec) => format!(
                "no graph matches {spec:?} under {}; available: {}",
                home.display(),
                if ids.is_empty() {
                    "none".into()
                } else {
                    ids.join(", ")
                }
            ),
            None => format!(
                "no graph under {}: workspace {} has no snapshot. Build one with graphBuild for the path, then query it with the same path",
                home.display(),
                home.parent()
                    .and_then(Path::parent)
                    .unwrap_or(&home)
                    .display()
            ),
        };
        fail(ErrorCode::NotFound, message)
    })
}

/// The snapshot `spec` names among `ids` (oldest first): the id itself, else
/// the newest snapshot of the one root whose scope is `spec`, else the
/// newest snapshot of the one root whose ids contain `spec`. A scope or
/// substring that names several roots is ambiguous and lists them; it never
/// picks one silently.
pub(super) fn choose_snapshot(
    home: &Path,
    ids: &[String],
    spec: &str,
) -> Result<Option<String>, Failure> {
    if ids.iter().any(|id| id == spec) {
        return Ok(Some(spec.to_owned()));
    }
    let manifests = ids
        .iter()
        .filter_map(|id| Some((id, super::super::read_manifest(&home.join(id))?)))
        .collect::<Vec<_>>();
    let scoped = manifests
        .iter()
        .filter(|(_, manifest)| manifest["scope"] == spec)
        .collect::<Vec<_>>();
    if !scoped.is_empty() {
        // Newest snapshot of each root carrying that scope.
        let mut newest = BTreeMap::<String, &String>::new();
        for (id, manifest) in &scoped {
            let root = manifest["root"].as_str().unwrap_or_default().to_owned();
            newest.insert(root, id);
        }
        let candidates = newest.into_values().cloned().collect::<Vec<_>>();
        return match candidates.as_slice() {
            [only] => Ok(Some(only.clone())),
            _ => Err(ambiguous_graph(spec, &candidates)),
        };
    }
    let matching = manifests
        .iter()
        .filter(|(id, _)| id.contains(spec))
        .collect::<Vec<_>>();
    let roots = matching
        .iter()
        .map(|(_, manifest)| manifest["root"].as_str().unwrap_or_default())
        .collect::<std::collections::BTreeSet<_>>();
    match matching.as_slice() {
        [] => Ok(None),
        // Oldest first: the last match is that root's newest snapshot.
        [.., (newest, _)] if roots.len() == 1 => Ok(Some((*newest).clone())),
        _ => Err(ambiguous_graph(
            spec,
            &matching
                .iter()
                .map(|(id, _)| (*id).clone())
                .collect::<Vec<_>>(),
        )),
    }
}

pub(super) fn ambiguous_graph(spec: &str, candidates: &[String]) -> Failure {
    let mut out = GraphOutput::error(
        ErrorCode::InvalidInput,
        format!(
            "{} graphs match {spec:?}; pass one exact snapshot id as graph",
            candidates.len()
        ),
    );
    out.value["candidates"] = json!(candidates);
    Box::new(out)
}

pub(super) fn load(
    options: &QueryOptions,
    paths: &PathPolicy,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Result<Graph, Failure> {
    load_with(options, paths, cancel, true)
}

pub(super) fn load_with(
    options: &QueryOptions,
    paths: &PathPolicy,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
    selective: bool,
) -> Result<Graph, Failure> {
    let (dir, explicit_path) = locate(options)?;
    let dir = paths
        .validate(&dir)
        .map_err(|error| fail(error.shared_code(), error.message))?
        .canonical;
    let manifest = std::fs::read(dir.join(MANIFEST_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .ok_or_else(|| {
            fail(
                ErrorCode::Decode,
                format!("{} has no readable manifest", dir.display()),
            )
        })?;
    let valid_registry = manifest["registryIdentity"]
        .as_str()
        .is_some_and(|identity| {
            identity.len() == 64
                && identity
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        && manifest["registryExtensions"]
            .as_array()
            .is_some_and(|extensions| {
                extensions.iter().all(|extension| {
                    extension
                        .as_str()
                        .is_some_and(|extension| !extension.is_empty())
                })
            });
    if !valid_registry {
        return Err(fail(
            ErrorCode::Decode,
            "graph manifest has no valid captured registry inventory; rebuild it with graphBuild",
        ));
    }
    // Every later read, hash, mention scan and `git -C` runs under the
    // manifest's root, which its digest does not cover: it must be the
    // canonical spelling of a directory inside the allowed roots.
    let root = manifest["root"].as_str().unwrap_or_default();
    let allowed = paths
        .validate_output(root)
        .map_err(|error| fail(error.shared_code(), error.message))?;
    if allowed.canonical != Path::new(root) {
        return Err(fail(
            ErrorCode::Decode,
            format!(
                "{MANIFEST_FILE} root {root:?} is not a canonical path; rebuild it with graphBuild"
            ),
        ));
    }
    let expected = manifest["graph"]["sha256"]
        .as_str()
        .ok_or_else(|| fail(ErrorCode::Decode, "graph manifest digest is missing"))?;
    let point = selective && matches!(options.op.as_str(), "node" | "find" | "candidates");
    let evidence = matches!(options.op.as_str(), "witnesses" | "unresolved");
    let tables_kind = if evidence {
        format!("evidence:{}", options.op)
    } else if point {
        format!("point:{}", matches!(options.op.as_str(), "node" | "find"))
    } else {
        "topology".to_owned()
    };
    let reuse = (selective && options.reuse_snapshot)
        .then(|| super::opened::Key::of(&dir, expected, tables_kind))
        .flatten();
    let cached = reuse.as_ref().and_then(super::opened::check_out);
    let (t, selective, validation) = if let Some((tables, reader)) = cached {
        (tables, Some(std::cell::RefCell::new(reader)), None)
    } else if selective {
        let mut reader =
            super::super::format::SnapshotReader::open(&dir.join(GRAPH_FILE), expected, cancel)
                .map_err(|message| decode_failure(message, cancel))?;
        let tables = if evidence {
            reader.evidence_tables(options.op == "witnesses", cancel)
        } else if point {
            reader.point_tables(matches!(options.op.as_str(), "node" | "find"), cancel)
        } else {
            reader.topology_tables(cancel)
        }
        .map_err(|message| decode_failure(message, cancel))?;
        (tables, Some(std::cell::RefCell::new(reader)), None)
    } else {
        let bytes = read_graph_file(&dir.join(GRAPH_FILE))
            .map_err(|message| fail(ErrorCode::Decode, message))?;
        let (tables, digest) = decode_with_cancel(&bytes, cancel)
            .map_err(|message| decode_failure(message, cancel))?;
        if digest != expected {
            return Err(fail(
                ErrorCode::Decode,
                "graph.bin does not match its manifest digest; rebuild graph",
            ));
        }
        let validation = super::super::format::full_validation(&bytes, &tables)
            .map_err(|message| fail(ErrorCode::Decode, message))?;
        (tables, None, Some(validation))
    };
    let id = manifest["id"].as_str().unwrap_or_default().to_owned();
    #[cfg(not(test))]
    let _ = validation;
    Ok(Graph {
        explicit_path: explicit_path.then_some(dir),
        id,
        manifest,
        t,
        selective,
        #[cfg(test)]
        validation,
        reuse,
        ..Default::default()
    })
}

pub(super) fn decode_failure(
    message: String,
    cancel: &dyn crate::execution::cancel::CancellationCheck,
) -> Failure {
    if let Err(cancelled) = cancel.check() {
        fail(ErrorCode::Cancelled, cancelled)
    } else {
        fail(ErrorCode::Decode, message)
    }
}
