//! Choosing and loading the snapshot a query reads.
use super::*;

pub(super) fn locate(options: &QueryOptions) -> Result<PathBuf, Failure> {
    if let Some(spec) = options.graph.as_deref() {
        let path = PathBuf::from(spec);
        if path.is_file() {
            return Ok(path.parent().map(Path::to_path_buf).unwrap_or_default());
        }
        if path.join(GRAPH_FILE).is_file() {
            return Ok(path);
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
    chosen.map(|id| home.join(id)).ok_or_else(|| {
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
                "no graph under {}; run `octocode graph ingest <path>` first",
                home.display()
            ),
        };
        fail(ErrorCode::NotFound, message)
    })
}

/// The snapshot `spec` names among `ids` (oldest first): the id itself, else
/// the newest snapshot of the one root whose scope is `spec`, else the one
/// id containing `spec`. A scope or substring that names several roots or
/// ids is ambiguous and lists them; it never picks one silently.
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
    let matching = ids
        .iter()
        .filter(|id| id.contains(spec))
        .cloned()
        .collect::<Vec<_>>();
    match matching.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(only.clone())),
        _ => Err(ambiguous_graph(spec, &matching)),
    }
}

pub(super) fn ambiguous_graph(spec: &str, candidates: &[String]) -> Failure {
    let mut out = GraphOutput::error(
        ErrorCode::InvalidInput,
        format!(
            "{} graphs match {spec:?}; pass one exact id with --graph",
            candidates.len()
        ),
    );
    out.value["candidates"] = json!(candidates);
    Box::new(out)
}

pub(super) fn load(options: &QueryOptions, paths: &PathPolicy) -> Result<Graph, Failure> {
    let dir = locate(options)?;
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
    let bytes = read_graph_file(&dir.join(GRAPH_FILE)).map_err(|error| {
        fail(
            ErrorCode::Decode,
            format!("cannot read {GRAPH_FILE}: {error}"),
        )
    })?;
    let (t, digest) = decode(&bytes).map_err(|message| fail(ErrorCode::Decode, message))?;
    if manifest["graph"]["sha256"].as_str() != Some(digest.as_str()) {
        return Err(fail(
            ErrorCode::Decode,
            "graph.bin does not match its manifest digest; re-run `octocode graph ingest`",
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
                "{MANIFEST_FILE} root {root:?} is not a canonical path; re-run `octocode graph ingest`"
            ),
        ));
    }
    let id = manifest["id"].as_str().unwrap_or_default().to_owned();
    Ok(Graph { id, manifest, t })
}
