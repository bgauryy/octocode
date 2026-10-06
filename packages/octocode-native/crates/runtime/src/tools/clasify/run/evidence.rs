//! Provider evidence: the narrowed state a captured page sends the judge
//! (row data without control fields, file reads as `{path, lines, content}`),
//! and the host-path redaction applied before it leaves the host.
use super::*;
use crate::tools::clasify::resource::tool_of;

pub(super) fn fallback_context(source: &Value) -> Value {
    let digest = crate::digest::json_sha256(source);
    match source.get("tool").and_then(Value::as_str) {
        Some(tool) => json!({"source":"tool","tool":tool,"resultHash":digest,"coverage":"partial",
            "limitations":["Context retrieval failed before a complete page was captured."]}),
        None => json!({"source":"value","resultHash":digest,"coverage":"partial",
            "limitations":["Context retrieval failed before a complete page was captured."]}),
    }
}

/// Evidence the provider judges: row data without the response envelope or
/// control fields. The envelope's shared `root` stays so relative paths keep
/// their root.
pub(super) fn tool_payload(state: &Value) -> Value {
    let Some(rows) = state.get("results").and_then(Value::as_array) else {
        return state.clone();
    };
    let mut payload = rows
        .iter()
        .filter_map(|row| row.get("data"))
        .cloned()
        .collect::<Vec<_>>();
    for data in &mut payload {
        if let Some(fields) = data.as_object_mut() {
            fields.retain(|key, _| !CONTROL_FIELDS.contains(&key.as_str()));
        }
    }
    let payload = if payload.len() == 1 {
        payload.pop().unwrap_or(Value::Null)
    } else {
        Value::Array(payload)
    };
    match state.get("root") {
        Some(root) => json!({"root":root,"data":payload}),
        None => payload,
    }
}

/// `[start, end]` source lines of one file-read row, when reported.
pub(super) fn evidence_lines(data: &Value) -> Option<Value> {
    let ranges = data.get("sourceLineRanges").and_then(Value::as_array);
    if let Some(ranges) = ranges.filter(|ranges| ranges.len() > 1) {
        let pairs = ranges
            .iter()
            .map(|range| Some(json!([range.get("start")?, range.get("end")?])))
            .collect::<Option<Vec<_>>>()?;
        return Some(Value::Array(pairs));
    }
    let range = ranges.and_then(|ranges| ranges.first());
    if let Some(range) = range {
        return Some(json!([range.get("start")?, range.get("end")?]));
    }
    if let (Some(start), Some(end)) = (data.get("startLine"), data.get("endLine"))
        && data
            .get("contentView")
            .and_then(Value::as_str)
            .is_none_or(|view| view == "none")
    {
        return Some(json!([start, end]));
    }
    // A compacted view has its own line positions. Without explicit source
    // ranges, its offsets cannot be presented as source line coordinates.
    if data
        .get("contentView")
        .and_then(Value::as_str)
        .is_some_and(|view| view != "none")
    {
        return None;
    }
    // Redacted or complete reads may omit ranges; derive them from the line
    // pagination window, or the whole file when unpaginated.
    let returned = data
        .get("returnedLines")
        .or_else(|| data.get("totalLines"))?
        .as_u64()?;
    let offset = match data.get("pagination") {
        Some(pagination) if pagination["unit"] == "lines" => pagination["offset"].as_u64()?,
        Some(_) => return None,
        None => 0,
    };
    (returned > 0).then(|| json!([offset + 1, offset + returned]))
}

pub(super) fn evidence_entry(repo: Option<String>, data: &Value) -> Option<Value> {
    let content = data.get("content").and_then(Value::as_str)?;
    let mut entry = serde_json::Map::new();
    if let Some(repo) = repo {
        entry.insert("repo".into(), json!(repo));
    }
    entry.insert("path".into(), data.get("path")?.clone());
    if let Some(lines) = evidence_lines(data) {
        entry.insert("lines".into(), lines);
    }
    entry.insert("content".into(), json!(content));
    Some(Value::Object(entry))
}

/// File reads judged as evidence only: `{repo?, path, lines, content}`. Read
/// metadata (absolute base, timestamps, byte counters, pagination, file type)
/// cost ~26% extra provider tokens and budget without informing a verdict.
pub(super) fn file_evidence(state: &Value) -> Option<Value> {
    let mut entries = Vec::new();
    for data in state
        .get("results")?
        .as_array()?
        .iter()
        .filter_map(|row| row.get("data"))
    {
        let repo = match (data["owner"].as_str(), data["repo"].as_str()) {
            (Some(owner), Some(repo)) => Some(format!("{owner}/{repo}")),
            _ => None,
        };
        entries.extend(evidence_entry(repo, data));
    }
    match entries.len() {
        0 => None,
        1 => entries.pop(),
        _ => Some(Value::Array(entries)),
    }
}

pub(super) fn is_file_read(source: &Value) -> bool {
    tool_of(source).is_some_and(clasify::is_file_read_tool)
}

/// A direct file read's own call, pinned to the returned ref when the caller
/// named none. Located pages and `best` rows narrow it to one exact
/// window; the receipt keeps it private (`fileRead`), never published whole.
pub(super) fn file_read_template(source: &Value, receipt: &Value) -> Option<Value> {
    let tool = tool_of(source).filter(|tool| clasify::is_file_read_tool(*tool))?;
    let mut query = source
        .get("query")
        .filter(|query| query.is_object())?
        .clone();
    if tool == ToolId::GhGetFileContent
        && query.get("ref").is_none()
        && let Some(reference) = receipt.pointer("/source/ref").filter(|r| r.is_string())
    {
        query["ref"] = reference.clone();
    }
    Some(json!({"tool":tool.as_str(),"query":query}))
}

/// Path-valued fields of local tool envelopes and queries.
pub(super) const LOCAL_PATH_FIELDS: [&str; 3] = ["root", "path", "workspaceRoot"];

/// An absolute local path as the provider sees it: workspace-relative, else
/// the file name. `None` leaves non-absolute values (GitHub paths, refs) as is.
pub(super) fn provider_path(paths: &PathPolicy, value: &str) -> Option<String> {
    let path = Path::new(value.strip_prefix("file://").unwrap_or(value));
    if !path.is_absolute() {
        return None;
    }
    let shown = paths.redact(path);
    if shown.is_empty() || shown == "~" || shown.starts_with("~/") {
        return Some(path.file_name().map_or_else(
            || ".".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        ));
    }
    Some(shown)
}

/// Provider state and read briefs leave the host: absolute local paths
/// (envelope `root`, query `path`/`workspaceRoot`) would disclose the
/// user's directory layout to the external classifier.
pub(super) fn relativize_local_paths(value: &mut Value, paths: &PathPolicy) {
    match value {
        Value::Object(fields) => {
            for (key, field) in fields.iter_mut() {
                if LOCAL_PATH_FIELDS.contains(&key.as_str())
                    && let Some(shown) = field.as_str().and_then(|raw| provider_path(paths, raw))
                {
                    *field = Value::String(shown);
                } else {
                    relativize_local_paths(field, paths);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                relativize_local_paths(item, paths);
            }
        }
        _ => {}
    }
}

pub(super) fn provider_state(source: &Value, state: Value) -> Value {
    if source.get("value").is_some() {
        state
    } else if let Some(evidence) = is_file_read(source)
        .then(|| file_evidence(&state))
        .flatten()
    {
        evidence
    } else {
        tool_payload(&state)
    }
}

/// One candidate split from a list page: its row without the list's paging
/// metadata or snippet highlight offsets, which do not inform its verdict.
pub(super) fn candidate_state(source: &Value, state: Value) -> Value {
    let mut payload = provider_state(source, state);
    let data = if payload.get("root").is_some() {
        &mut payload["data"]
    } else {
        &mut payload
    };
    if let Some(fields) = data.as_object_mut() {
        fields.remove("pagination");
        fields.remove("effectiveQuery");
        for file in fields
            .get_mut("files")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            for matched in file
                .get_mut("matches")
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                if let Some(matched) = matched.as_object_mut() {
                    matched.remove("matchIndices");
                }
            }
        }
    }
    payload
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_paths_are_workspace_relative_or_file_names() {
        let root = std::env::temp_dir().join("octocode-clasify-paths");
        let paths = PathPolicy::new(crate::policy::path::PathPolicyConfig {
            workspace_root: Some(root.clone()),
            home_dir: Some(std::path::PathBuf::from("/home/someone")),
            ..Default::default()
        })
        .expect("policy");
        let inside = root.join("src/a.rs").to_string_lossy().into_owned();
        let mut state = json!({
            "root": root.to_string_lossy(),
            "results":[{"data":{"path":"src/a.rs"}}],
            "query":{"path":inside,"workspaceRoot":"/elsewhere/proj"},
            "home":{"path":"/home/someone/notes/b.md"},
            "gh":{"path":"docs/c.md","base":"main"}
        });
        relativize_local_paths(&mut state, &paths);
        assert_eq!(state["root"], ".");
        assert_eq!(state["results"][0]["data"]["path"], "src/a.rs");
        assert_eq!(state["query"]["path"], "src/a.rs");
        assert_eq!(state["query"]["workspaceRoot"], "proj");
        assert_eq!(state["home"]["path"], "b.md");
        assert_eq!(state["gh"], json!({"path":"docs/c.md","base":"main"}));
    }

    #[test]
    fn disjoint_file_evidence_keeps_distinct_source_ranges() {
        let state = json!({"results":[{"data":{
            "path":"/tmp/example.rs","content":(0..100).map(|i| format!("line {i}\n")).collect::<String>(),
            "sourceLineRanges":[{"start":4,"end":53},{"start":1000,"end":1049}]
        }}]});
        let evidence = file_evidence(&state).expect("file evidence");
        assert_eq!(evidence["lines"], json!([[4, 53], [1000, 1049]]));
    }

    #[test]
    fn transformed_symbols_are_not_treated_as_original_source_lines() {
        let content = (1..=100)
            .map(|line| format!("{line}\t## Heading\n"))
            .collect::<String>();
        let state = json!({"results":[{"data":{
            "path":"Hooks.md", "content":content,
            "contentView":"symbols", "totalLines":954, "returnedLines":100
        }}]});
        let evidence = file_evidence(&state).expect("file evidence");
        assert!(evidence.get("lines").is_none());
    }

    #[test]
    fn provider_payload_keeps_evidence_and_root_but_drops_control_fields() {
        let state = json!({"root":"/repo","results":[{"index":0,"meta":{"x":1},"data":{
            "symbols":[{"name":"apply_row"}],
            "diagnostics":[{"code":"scan.skipped"}],
            "hints":["Try lspSearch"],
            "next":{"continue":{"tool":"localFetch","query":{"path":"a"}}}
        }}]});
        let payload = tool_payload(&state);
        assert_eq!(
            payload,
            json!({"root":"/repo","data":{"symbols":[{"name":"apply_row"}]}})
        );
        let source = json!({"tool":"astSearch","query":{}});
        let bare = json!({"results":[{"data":{"symbols":[{"name":"apply_row"}]}}]});
        assert_eq!(
            assessed_payload_chars(&source, &state),
            assessed_payload_chars(&source, &bare),
            "diagnostics and hints must not consume the evidence budget"
        );
    }

    #[test]
    fn candidate_pages_drop_list_paging_and_highlight_offsets() {
        let history = json!({"tool":"ghSearchHistory","query":{"operation":"pullRequest"}});
        let item = json!({"results":[{"data":{
            "type":"pullRequests","pullRequests":[{"number":7,"title":"mpsc: release permits"}],
            "effectiveQuery":"mpsc is:pr","pagination":{"currentPage":1,"hasMore":true}
        }}]});
        assert_eq!(
            candidate_state(&history, item),
            json!({"type":"pullRequests","pullRequests":[{"number":7,"title":"mpsc: release permits"}]})
        );
        let code = json!({"tool":"ghSearchCode","query":{"owner":"o"}});
        let hit = json!({"results":[{"data":{"files":[{"owner":"o","repo":"r","path":"a.rs",
            "matches":[{"value":"acquire(n)","matchIndices":[{"start":0,"end":7}]}]}],
            "pagination":{"currentPage":1}}}]});
        assert_eq!(
            candidate_state(&code, hit),
            json!({"files":[{"owner":"o","repo":"r","path":"a.rs","matches":[{"value":"acquire(n)"}]}]})
        );
    }

    #[test]
    fn file_reads_reach_the_provider_as_evidence_only() {
        let local = json!({"root":"/abs/root","results":[{"data":{
            "path":"a.rs","content":"fn a() {}\n","totalLines":1,"modified":"2026",
            "sourceLineRanges":[{"start":1,"end":1}],"sourceBytes":10,"fileType":"code",
            "next":{"continue":{}}}}]});
        let source = json!({"tool":"localFetch","query":{}});
        assert_eq!(
            provider_state(&source, local.clone()),
            json!({"path":"a.rs","lines":[1,1],"content":"fn a() {}\n"})
        );
        assert_eq!(assessed_payload_chars(&source, &local), 10);
        let github = json!({"results":[{"data":{"owner":"o","repo":"r",
            "path":"b.js","content":"x","startLine":3,"endLine":3,"commitSha":"sha"}}]});
        assert_eq!(
            provider_state(&json!({"tool":"ghGetFileContent","query":{}}), github),
            json!({"repo":"o/r","path":"b.js","lines":[3,3],"content":"x"})
        );
    }

    #[test]
    fn evidence_lines_fall_back_to_the_pagination_window() {
        let redacted = json!({"returnedLines":100,
            "pagination":{"unit":"lines","offset":600,"length":100}});
        assert_eq!(evidence_lines(&redacted), Some(json!([601, 700])));
        let whole = json!({"totalLines":42});
        assert_eq!(evidence_lines(&whole), Some(json!([1, 42])));
        let bytes = json!({"returnedLines":5,"pagination":{"unit":"bytes","offset":10}});
        assert_eq!(evidence_lines(&bytes), None);
    }
}
