use crate::tools::id::ToolId;
use serde_json::{Value, json};

/// Encoding of the rendered text channel, selected by `output.format`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextFormat {
    #[default]
    Yaml,
    Json,
}

impl TextFormat {
    /// Map the resolved `output.format` value; the contract admits only
    /// `yaml` and `json`, so anything else keeps the YAML default.
    #[must_use]
    pub fn from_config(value: &str) -> Self {
        if value == "json" {
            Self::Json
        } else {
            Self::Yaml
        }
    }

    fn encode(self, mut value: Value, keys: &[&str]) -> String {
        match self {
            Self::Yaml => yaml(flatten_single_row(value), keys),
            Self::Json => {
                order_fields(&mut value, keys);
                serde_json::to_string(&value).unwrap_or_default()
            }
        }
    }
}

/// Result families the text channel renders differently. Each tool belongs
/// to exactly one; the match is exhaustive, so a new tool must pick one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RenderFamily {
    /// Source text a caller copies from. Local reads carry one inline
    /// `content`; GitHub reads carry a `files[]` list.
    FileText(FileLayout),
    /// Ranked search hits (files, match rows, page cursor).
    SearchHits,
    /// History items: metadata, then each changed file's patch verbatim.
    Diff,
    /// astSearch: symbols rows render as an indented declaration outline.
    Outline,
    /// Every other result: generic ordered encoding.
    Structured,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileLayout {
    Inline,
    Files,
}

impl RenderFamily {
    const fn of(tool: ToolId) -> Self {
        match tool {
            ToolId::LocalFetch => Self::FileText(FileLayout::Inline),
            ToolId::GhGetFileContent => Self::FileText(FileLayout::Files),
            ToolId::LocalSearch => Self::SearchHits,
            ToolId::GhGetHistoryItem => Self::Diff,
            ToolId::AstSearch => Self::Outline,
            ToolId::GhSearchRepo
            | ToolId::GhSearchCode
            | ToolId::GhStructure
            | ToolId::GhSearchHistory
            | ToolId::GhCloneRepo
            | ToolId::ArtifactSearch
            | ToolId::StructureSearch
            | ToolId::AstTopology
            | ToolId::AstRewrite
            | ToolId::LspSearch
            | ToolId::Clasify => Self::Structured,
        }
    }
}

pub fn render_tool(tool: ToolId, response: &Value, query: &Value, format: TextFormat) -> String {
    let family = RenderFamily::of(tool);
    match family {
        RenderFamily::FileText(FileLayout::Inline) => render_inline_file(response, format),
        RenderFamily::FileText(FileLayout::Files) => render_file_list(response.clone(), format),
        RenderFamily::SearchHits => render_search_hits(response.clone(), query, format),
        RenderFamily::Diff => render_diff(response.clone(), format),
        RenderFamily::Outline => render_outline(response.clone(), format),
        RenderFamily::Structured => render_structured(response.clone(), format),
    }
}

fn render_inline_file(response: &Value, format: TextFormat) -> String {
    match format {
        TextFormat::Yaml => render_local_fetch(response),
        TextFormat::Json => format.encode(response.clone(), &["base", "results", "shared"]),
    }
}

fn render_search_hits(mut response: Value, query: &Value, format: TextFormat) -> String {
    if format == TextFormat::Yaml {
        compact_path_rows(&mut response);
    }
    for row in response
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(data) = row.get_mut("data") else {
            continue;
        };
        for file in data
            .get_mut("files")
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            order_fields(
                file,
                &[
                    "path",
                    "totalOccurrences",
                    "totalMatchedLines",
                    "totalMatchRows",
                    "returnedMatchRows",
                    "matches",
                    "pagination",
                ],
            );
        }
        if let Some(page) = data.get_mut("pagination") {
            order_fields(
                page,
                &[
                    "currentPage",
                    "totalPages",
                    "filesPerPage",
                    "totalFiles",
                    "totalMatches",
                    "hasMore",
                    "nextPage",
                    "outOfRange",
                    "snapshot",
                ],
            );
        }
        if query.get("snapshot").is_some()
            && let Some(page) = data.get_mut("pagination")
        {
            order_fields(
                page,
                &[
                    "snapshot",
                    "currentPage",
                    "totalPages",
                    "filesPerPage",
                    "totalFiles",
                    "totalMatches",
                    "hasMore",
                    "nextPage",
                    "outOfRange",
                ],
            );
        }
        if let Some(next) = data.get_mut("next").and_then(Value::as_object_mut) {
            for call in next.values_mut() {
                if let Some(query) = call.get_mut("query") {
                    order_fields(
                        query,
                        &[
                            "searchText",
                            "path",
                            "regex",
                            "caseMode",
                            "wholeWord",
                            "invertMatch",
                            "include",
                            "exclude",
                            "excludeDir",
                            "noIgnore",
                            "hidden",
                            "contextLines",
                            "matchContentLength",
                            "maxMatchesPerFile",
                            "maxFiles",
                            "maxDepth",
                            "multiline",
                            "sort",
                            "langType",
                            "unique",
                            "matchWindow",
                            "matchPage",
                            "page",
                            "snapshot",
                            "resultView",
                            "pageSize",
                            "reverse",
                        ],
                    );
                }
            }
        }
    }
    render_structured(response, format)
}

/// Search rows that carry only a `path` (`resultView:"files"`), or a path and
/// its one count (`countMatches`/`countLines`), render as one string each:
/// `path` or `path (count)`.
fn compact_path_rows(response: &mut Value) {
    for files in response
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
        .filter_map(|row| row.get_mut("data")?.get_mut("files")?.as_array_mut())
    {
        for file in files {
            let Some(map) = file.as_object() else {
                continue;
            };
            let Some(path) = map.get("path").and_then(Value::as_str) else {
                continue;
            };
            let count = ["totalOccurrences", "totalMatchedLines"]
                .iter()
                .find_map(|key| map.get(*key).and_then(Value::as_u64));
            let compact = match (map.len(), count) {
                (1, _) => path.to_owned(),
                (2, Some(count)) => format!("{path} ({count})"),
                _ => continue,
            };
            *file = Value::String(compact);
        }
    }
}

fn render_file_list(mut response: Value, format: TextFormat) -> String {
    for row in response
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        for file in row
            .get_mut("data")
            .and_then(|data| data.get_mut("files"))
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            order_fields(
                file,
                &[
                    "path",
                    "content",
                    "sourceLineRanges",
                    "errorCode",
                    "terminalLimit",
                    "partialReasons",
                    "fileType",
                    "contentView",
                    "totalLines",
                    "sourceChars",
                    "sourceBytes",
                    "returnedChars",
                    "returnedBytes",
                    "returnedLines",
                    "selectedMatchCount",
                    "minifyFallback",
                    "resolvedBranch",
                    "commitSha",
                    "pagination",
                    "next",
                    "isPartial",
                    "startLine",
                    "endLine",
                    "matchRanges",
                    "matchedLines",
                    "lastModified",
                    "lastModifiedBy",
                    "warnings",
                    "matchNotFound",
                    "searchedFor",
                    "cached",
                ],
            );
            if matches!(
                file["errorCode"].as_str(),
                Some("fullContentLimit" | "noMatches")
            ) {
                order_fields(
                    file,
                    &[
                        "path",
                        "content",
                        "fileType",
                        "contentView",
                        "totalLines",
                        "sourceChars",
                        "sourceBytes",
                        "returnedChars",
                        "returnedBytes",
                        "returnedLines",
                        "errorCode",
                        "partialReasons",
                        "commitSha",
                        "next",
                        "isPartial",
                    ],
                );
            }
            if let Some(next) = file.get_mut("next").and_then(Value::as_object_mut) {
                for call in next.values_mut() {
                    if let Some(query) = call.get_mut("query") {
                        order_fields(
                            query,
                            &[
                                "owner",
                                "repo",
                                "path",
                                "branch",
                                "matchString",
                                "matchStringIsRegex",
                                "matchStringCaseSensitive",
                                "startLine",
                                "endLine",
                                "contextLines",
                                "contextBytes",
                                "chunkType",
                                "offset",
                                "chunkSize",
                                "fullContent",
                                "forceRefresh",
                                "minify",
                            ],
                        );
                    }
                }
            }
        }
    }
    let mut blocks = Vec::new();
    if format == TextFormat::Yaml {
        take_file_contents(&mut response, &mut blocks);
    }
    let mut text = format.encode(
        response,
        &[
            "base",
            "shared",
            "results",
            "index",
            "status",
            "meta",
            "data",
            "owner",
            "repo",
            "files",
            "path",
            "content",
            "fileType",
            "totalLines",
            "startLine",
            "endLine",
            "isPartial",
            "pagination",
            "error",
        ],
    );
    for (header, content) in blocks {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&header);
        text.push('\n');
        text.push_str(content.strip_suffix('\n').unwrap_or(&content));
        text.push('\n');
    }
    text
}

/// GitHub file text in YAML: a multi-line `content` would be a quoted scalar
/// escaping every tab, quote and line break. Each file's content leaves the
/// YAML and follows it verbatim under a header naming the file, numbered
/// `<line>\t<text>` when its lines map onto source lines (C5), as localFetch
/// renders its own `content (source lines):` block.
fn take_file_contents(response: &mut Value, blocks: &mut Vec<(String, String)>) {
    let rows = response
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .map(|rows| rows.as_mut_slice())
        .unwrap_or_default();
    let single = rows.len() == 1
        && rows[0]["data"]["files"]
            .as_array()
            .is_some_and(|files| files.len() == 1);
    for row in rows {
        let index = row["index"].clone();
        for file in row
            .get_mut("data")
            .and_then(|data| data.get_mut("files"))
            .and_then(Value::as_array_mut)
            .into_iter()
            .flatten()
        {
            let numbered = super::numbered::numbered_view(file);
            let Some(map) = file.as_object_mut() else {
                continue;
            };
            let Some(content) = map
                .get("content")
                .and_then(Value::as_str)
                .filter(|content| !content.is_empty())
                .map(str::to_owned)
            else {
                continue;
            };
            map.remove("content");
            let label = if numbered.is_some() {
                map.remove("sourceLineRanges");
                "source lines"
            } else {
                "copy-safe"
            };
            let header = if single {
                format!("content ({label}):")
            } else {
                let path = map.get("path").and_then(Value::as_str).unwrap_or("");
                format!("=== [{index}] {path} content ({label}) ===")
            };
            blocks.push((header, numbered.unwrap_or(content)));
        }
    }
}

fn render_structured(response: Value, format: TextFormat) -> String {
    format.encode(
        response,
        &[
            "base",
            "shared",
            "results",
            "index",
            "status",
            "cache",
            "meta",
            "evidence",
            "diagnostics",
            "data",
        ],
    )
}

/// History items in YAML: a patch inside YAML is a double-quoted scalar that
/// escapes every line break, carriage return, quote and backslash (5–15% of a
/// diff). Each non-empty patch leaves the YAML with its changed-file row and
/// follows the metadata verbatim under one header that folds the row in:
/// `=== patch M +3 -1 path[ <- old/path] (<span>) ===`, where the span is
/// `n chars`, `n of F chars` (a `matchString` view of an F-char patch) or
/// `chars a-b of T` (a window). Diff lines start with ` `, `+`, `-`, `@` or
/// `\`, so a header cannot be mistaken for patch text. Structured content
/// keeps every row; JSON text keeps the structured encoding unchanged.
fn render_diff(mut response: Value, format: TextFormat) -> String {
    if format == TextFormat::Json {
        return render_structured(response, format);
    }
    let rows = response
        .get_mut("results")
        .and_then(Value::as_array_mut)
        .map(|rows| rows.as_mut_slice())
        .unwrap_or_default();
    let labelled = rows.len() > 1;
    let mut patches = Vec::new();
    for row in rows {
        let label = labelled.then(|| format!("[{}] ", row["index"]));
        take_patches(
            &mut row["data"],
            label.as_deref().unwrap_or(""),
            &mut patches,
        );
    }
    let mut text = render_structured(response, format);
    for (header, patch) in patches {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("=== patch {header} ===\n{patch}"));
    }
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

/// astSearch in YAML: symbols declarations leave the YAML and follow it as
/// one outline section per file (see `symbol_outline`). JSON text and
/// structured content keep the rows unchanged.
fn render_outline(mut response: Value, format: TextFormat) -> String {
    if format == TextFormat::Json {
        return render_structured(response, format);
    }
    let sections = super::symbol_outline::take_outlines(&mut response);
    let mut text = render_structured(response, format);
    for section in sections {
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&section);
    }
    text
}

/// Move changed-file rows with a non-empty `patch` (`changedFiles`, `files`)
/// out of the YAML in document order; `next.*` continuation queries stay.
fn take_patches(value: &mut Value, label: &str, out: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            let mut emptied = Vec::new();
            for (key, child) in map.iter_mut() {
                if key == "next" {
                    continue;
                }
                if matches!(key.as_str(), "changedFiles" | "files")
                    && let Some(rows) = child.as_array_mut()
                {
                    rows.retain(|row| match patch_section(row, label) {
                        Some(section) => {
                            out.push(section);
                            false
                        }
                        None => true,
                    });
                    if rows.is_empty() {
                        emptied.push(key.clone());
                    }
                }
                take_patches(child, label, out);
            }
            for key in emptied {
                map.remove(&key);
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| take_patches(item, label, out)),
        _ => {}
    }
}

/// The `(header, patch)` section of a changed-file row with a non-empty patch.
fn patch_section(row: &Value, label: &str) -> Option<(String, String)> {
    let patch = row
        .get("patch")?
        .as_str()
        .filter(|patch| !patch.is_empty())?;
    let text = |key: &str| row.get(key).and_then(Value::as_str);
    let count = |key: &str| row.get(key).and_then(Value::as_u64);
    let mut header = label.to_owned();
    if let Some(status) = text("status") {
        let code = match status {
            "added" => "A",
            "removed" => "D",
            "modified" => "M",
            "renamed" => "R",
            "copied" => "C",
            "changed" => "T",
            "unchanged" => "U",
            other => other,
        };
        header.push_str(&format!(
            "{code} +{} -{} ",
            count("additions").unwrap_or(0),
            count("deletions").unwrap_or(0)
        ));
    }
    header.push_str(text("path").or_else(|| text("filename")).unwrap_or(""));
    if let Some(previous) = text("previousPath").or_else(|| text("previousFilename")) {
        header.push_str(&format!(" <- {previous}"));
    }
    let chars = patch.chars().count();
    let span = match (row.get("patchPagination"), count("fullPatchChars")) {
        (Some(page), _) => {
            let from = page.get("charOffset").and_then(Value::as_u64).unwrap_or(0);
            let total = page.get("totalChars").and_then(Value::as_u64).unwrap_or(0);
            format!("chars {from}-{} of {total}", from + chars as u64)
        }
        (None, Some(full)) => format!("{chars} of {full} chars"),
        (None, None) => format!("{chars} chars"),
    };
    header.push_str(&format!(" ({span})"));
    Some((header, patch.to_owned()))
}

/// YAML text of a single-row response: the row's `results: - index: 0 data:`
/// wrapper names nothing the one query does not, so the envelope fields, the
/// row's own fields (`status`, `meta`, ...), and its `data` fields render at
/// the top. A batch keeps the wrapper, whose `index` binds a row to its query,
/// and so does a row whose data would shadow an envelope or row field.
/// Structured content and JSON text keep the envelope unchanged.
fn flatten_single_row(value: Value) -> Value {
    let Value::Object(mut envelope) = value else {
        return value;
    };
    let single = match envelope.get("results").and_then(Value::as_array) {
        Some(rows) if rows.len() == 1 => rows[0].as_object().cloned(),
        _ => None,
    };
    let Some(mut row) = single else {
        return Value::Object(envelope);
    };
    let data = match row.remove("data") {
        None => serde_json::Map::new(),
        Some(Value::Object(data)) => data,
        Some(_) => return Value::Object(envelope),
    };
    row.remove("index");
    let mut flat = serde_json::Map::new();
    for (key, field) in envelope
        .iter()
        .filter(|(key, _)| *key != "results")
        .chain(&row)
        .chain(&data)
    {
        if flat.insert(key.clone(), field.clone()).is_some() {
            return Value::Object(envelope);
        }
    }
    envelope.clear();
    Value::Object(flat)
}

fn order_fields(value: &mut Value, keys: &[&str]) {
    if let Some(map) = value.as_object_mut() {
        let mut ordered = serde_json::Map::new();
        for key in keys {
            if let Some(value) = map.remove(*key) {
                ordered.insert((*key).into(), value);
            }
        }
        ordered.append(map);
        *map = ordered;
    }
}

fn yaml(value: Value, keys: &[&str]) -> String {
    octocode_engine::portable::json_to_yaml_string(
        value,
        Some(octocode_engine::types::YamlConversionConfig {
            sort_keys: Some(false),
            keys_priority: Some(keys.iter().map(|key| (*key).into()).collect()),
        }),
    )
}

pub fn render_local_fetch(response: &Value) -> String {
    let mut lines = Vec::new();
    if let Some(base) = response["base"].as_str() {
        lines.push(format!("base: {base}"));
    }
    let rows = response["results"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default();
    for (position, row) in rows.iter().enumerate() {
        let data = &row["data"];
        let mut metadata = data.clone();
        let numbered = super::numbered::numbered_view(data);
        if let Some(map) = metadata.as_object_mut() {
            map.remove("content");
            // Numbered lines state their own source range.
            if numbered.is_some() {
                map.remove("sourceLineRanges");
            }
        }
        order_read_metadata(&mut metadata);
        if position > 0 {
            lines.push(String::new());
        }
        // A sole row binds to the sole query; a batch names each row.
        if rows.len() > 1 || row.get("status").is_some() {
            let status = row["status"]
                .as_str()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            lines.push(format!("result: {}{status}", row["index"]));
        }
        let formatted = yaml(
            metadata,
            &[
                "path",
                "resolvedPath",
                "contentView",
                "startLine",
                "endLine",
                "totalLines",
                "isPartial",
                "pagination",
                "sourceChars",
                "sourceBytes",
                "returnedChars",
                "fileType",
                "warnings",
                "error",
            ],
        );
        let formatted = formatted.trim_end();
        if !formatted.is_empty() && formatted != "{}" {
            lines.push(formatted.into());
        }
        match (numbered, data["content"].as_str()) {
            (Some(numbered), _) => {
                lines.push("content (source lines):".into());
                lines.push(numbered.strip_suffix('\n').unwrap_or(&numbered).into());
            }
            (None, Some(content)) => {
                lines.push("content (copy-safe):".into());
                lines.push(content.strip_suffix('\n').unwrap_or(content).into());
            }
            (None, None) => {}
        }
    }
    if let Some(shared) = response.get("shared") {
        lines.push(
            yaml(json!({"shared":shared}), &["shared"])
                .trim_end()
                .into(),
        );
    }
    lines.join("\n") + "\n"
}

fn order_read_metadata(data: &mut Value) {
    let Some(map) = data.as_object_mut() else {
        return;
    };
    let mut ordered = serde_json::Map::new();
    for key in [
        "path",
        "returnedChars",
        "returnedBytes",
        "returnedLines",
        "pagination",
        "isPartial",
        "partialReasons",
        "terminalLimit",
        "next",
        "contentView",
        "sourceLineRanges",
        "totalLines",
        "startLine",
        "endLine",
        "matchRanges",
        "selectedMatchCount",
        "matchedLines",
        "modified",
        "warnings",
        "sourceChars",
        "sourceBytes",
        "fileType",
    ] {
        if let Some(value) = map.remove(key) {
            ordered.insert(key.into(), value);
        }
    }
    ordered.append(map);
    *map = ordered;
    if let Some(next) = map.get_mut("next").and_then(Value::as_object_mut) {
        for call in next.values_mut() {
            if let Some(query) = call.get_mut("query").and_then(Value::as_object_mut)
                && let Some(offset) = query.remove("offset")
            {
                query.insert("offset".into(), offset);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_format_json_renders_parseable_json_text() {
        let response =
            json!({"results":[{"index":0,"status":"empty","data":{"path":"a"}}],"base":"/r"});
        for tool in ["localSearch", "localFetch", "ghGetFileContent"] {
            let text = render_tool(
                ToolId::from_name(tool).expect("known tool"),
                &response,
                &json!({}),
                TextFormat::Json,
            );
            let parsed: Value = serde_json::from_str(&text).expect("json text");
            assert_eq!(parsed, response, "{tool}");
            assert!(text.starts_with("{\"base\""), "{tool}: {text}");
            let yaml = render_tool(
                ToolId::from_name(tool).expect("known tool"),
                &response,
                &json!({}),
                TextFormat::Yaml,
            );
            assert!(
                serde_json::from_str::<Value>(&yaml).is_err(),
                "{tool}: {yaml}"
            );
        }
        assert_eq!(TextFormat::from_config("json"), TextFormat::Json);
        assert_eq!(TextFormat::from_config("yaml"), TextFormat::Yaml);
    }

    #[test]
    fn every_tool_renders_through_exactly_one_family() {
        for tool in ToolId::ALL {
            let expected = match tool {
                ToolId::LocalFetch => RenderFamily::FileText(FileLayout::Inline),
                ToolId::GhGetFileContent => RenderFamily::FileText(FileLayout::Files),
                ToolId::LocalSearch => RenderFamily::SearchHits,
                ToolId::GhGetHistoryItem => RenderFamily::Diff,
                ToolId::AstSearch => RenderFamily::Outline,
                _ => RenderFamily::Structured,
            };
            assert_eq!(RenderFamily::of(tool), expected, "{tool}");
        }
    }

    #[test]
    fn search_hits_order_rows_before_the_structured_encoding() {
        let response = json!({"results":[{"index":0,"data":{"files":[{"matches":[],"path":"a.rs"}],
            "pagination":{"hasMore":false,"currentPage":1}}}]});
        let text = render_tool(
            ToolId::from_name("localSearch").expect("known tool"),
            &response,
            &json!({}),
            TextFormat::Json,
        );
        assert!(text.contains(r#"{"path":"a.rs","matches":[]}"#), "{text}");
        assert!(
            text.contains(r#"{"currentPage":1,"hasMore":false}"#),
            "{text}"
        );
        // The structured family leaves the same rows in producer order.
        let plain = render_tool(
            ToolId::from_name("astSearch").expect("known tool"),
            &response,
            &json!({}),
            TextFormat::Json,
        );
        assert!(plain.contains(r#"{"matches":[],"path":"a.rs"}"#), "{plain}");
    }

    /// YAML escapes every `\n`, `\r`, quote and backslash of a patch (5–15%
    /// of a diff); history items print patches verbatim after the metadata.
    #[test]
    fn history_patches_render_verbatim_after_the_metadata() {
        let patch = "@@ -1 +1 @@\r\n-say(\"a\\b\")\r\n+say(\"c\")";
        let response = json!({"results":[{"index":0,"data":{"type":"pullRequests","pullRequests":[{
            "number":1,"changedFiles":[
                {"path":"src/a.ts","status":"modified","additions":1,"deletions":1,"patch":patch},
                {"path":"src/b.ts","status":"renamed","previousPath":"old/b.ts","additions":2,"deletions":0,
                 "fullPatchChars":900,"patch":"+b"},
                {"path":"src/c.ts","status":"added","additions":9,"deletions":0,"patch":"+c",
                 "patchPagination":{"charOffset":10,"charLength":2,"totalChars":30,"hasMore":true,"nextCharOffset":12}},
                {"path":"src/moved.ts","status":"renamed","patch":""},
                {"path":"big.ts","patchUnavailable":"tooLarge"}
            ]}],
            "next":{"continuePatch":{"tool":"ghGetHistoryItem","query":{"patch":"not a patch"}}}}}]});
        let text = render_tool(
            ToolId::GhGetHistoryItem,
            &response,
            &json!({}),
            TextFormat::Yaml,
        );
        let (yaml, patches) = text.split_once("\n=== patch ").expect("patch section");
        assert!(!yaml.contains("say("), "{yaml}");
        // A row whose patch follows is folded into that patch's header.
        for folded in [
            "src/a.ts",
            "src/b.ts",
            "src/c.ts",
            "fullPatchChars",
            "patchPagination",
        ] {
            assert!(!yaml.contains(folded), "{folded} repeated: {yaml}");
        }
        assert!(yaml.contains("path: big.ts"), "patchless rows stay: {yaml}");
        assert!(
            yaml.contains("patch: ''"),
            "empty rename diff stays inline: {yaml}"
        );
        assert!(
            yaml.contains("not a patch"),
            "continuations untouched: {yaml}"
        );
        assert_eq!(
            patches,
            format!(
                "M +1 -1 src/a.ts ({} chars) ===\n{patch}\n\
                 === patch R +2 -0 src/b.ts <- old/b.ts (2 of 900 chars) ===\n+b\n\
                 === patch A +9 -0 src/c.ts (chars 10-12 of 30) ===\n+c\n",
                patch.chars().count()
            )
        );
        // JSON text keeps the exact structured encoding.
        let json_text = render_tool(
            ToolId::GhGetHistoryItem,
            &response,
            &json!({}),
            TextFormat::Json,
        );
        assert_eq!(
            serde_json::from_str::<Value>(&json_text).expect("json"),
            response
        );
    }

    /// Every source line keeps its own number (a text page may start mid
    /// range, and a `@@ a-b @@` block header would leave such a page, or a
    /// source line that itself reads `@@ 1-2 @@`, unnumbered). The gutter is
    /// unpadded; the numbers make `sourceLineRanges`
    /// redundant in text, and a sole row needs no `result:`/`data:` wrapper.
    /// The gutter is the C5 `<line>\t` form structuredContent carries.
    #[test]
    fn local_fetch_numbers_each_line_with_a_bare_gutter_under_a_flat_header() {
        let row = |path: &str, start: u64| {
            json!({"data":{"path":path,"totalLines":900,"content":"fn a() {\n\n    @@ 1-2 @@\n",
                "sourceLineRanges":[{"start":start,"end":start + 2}]}})
        };
        let mut one = json!({"base":"/r","results":[row("a.rs", 279)]});
        one["results"][0]["index"] = json!(0);
        assert_eq!(
            render_local_fetch(&one),
            "base: /r\npath: a.rs\ntotalLines: 900\ncontent (source lines):\n\
             279\tfn a() {\n280\t\n281\t    @@ 1-2 @@\n"
        );
        // Content the response stage already numbered renders unchanged.
        let mut staged = one.clone();
        super::super::numbered::number_read_rows(ToolId::LocalFetch, &mut staged);
        assert_eq!(render_local_fetch(&staged), render_local_fetch(&one));
        let two = json!({"base":"/r","results":[
            {"index":0,"data":row("a.rs", 1)["data"]},
            {"index":1,"status":"error","data":{"path":"b.rs","error":"missing"}}]});
        assert_eq!(
            render_local_fetch(&two),
            "base: /r\nresult: 0\npath: a.rs\ntotalLines: 900\ncontent (source lines):\n\
             1\tfn a() {\n2\t\n3\t    @@ 1-2 @@\n\nresult: 1 (error)\npath: b.rs\nerror: missing\n"
        );
    }

    /// GitHub file text leaves the YAML (no escaped tabs/newlines) and follows
    /// it numbered, like localFetch; JSON text keeps the structured encoding.
    #[test]
    fn github_file_content_renders_numbered_after_the_metadata() {
        let file = |path: &str, start: u64| {
            json!({"path":path,"content":"def a():\n    \"x\"\n","totalLines":40,
                "sourceLineRanges":[{"start":start,"end":start + 1}],"commitSha":"abc"})
        };
        let one = json!({"results":[{"index":0,"data":{"owner":"o","repo":"r","files":[file("a.py", 7)]}}]});
        let mut staged = one.clone();
        super::super::numbered::number_read_rows(ToolId::GhGetFileContent, &mut staged);
        for response in [&one, &staged] {
            let text = render_tool(
                ToolId::GhGetFileContent,
                response,
                &json!({}),
                TextFormat::Yaml,
            );
            assert!(
                text.ends_with("content (source lines):\n7\tdef a():\n8\t    \"x\"\n"),
                "{text}"
            );
            assert!(!text.contains("sourceLineRanges"), "{text}");
            assert!(text.contains("path: a.py"), "{text}");
        }
        let two = json!({"results":[
            {"index":0,"data":{"owner":"o","repo":"r","files":[file("a.py", 1)]}},
            {"index":1,"data":{"owner":"o","repo":"r","files":[{"path":"b.py","content":"x\n","contentView":"symbols"}]}}]});
        let text = render_tool(ToolId::GhGetFileContent, &two, &json!({}), TextFormat::Yaml);
        assert!(
            text.contains("=== [0] a.py content (source lines) ===\n1\tdef a():\n"),
            "{text}"
        );
        assert!(
            text.ends_with("=== [1] b.py content (copy-safe) ===\nx\n"),
            "{text}"
        );
        let json_text = render_tool(
            ToolId::GhGetFileContent,
            &staged,
            &json!({}),
            TextFormat::Json,
        );
        assert_eq!(
            serde_json::from_str::<Value>(&json_text).expect("json"),
            staged
        );
    }

    /// A sole row carries no information in `results: - index: 0 data:`; its
    /// fields render at the top. A batch keeps the wrapper (index binds each
    /// row to its query), and so does a row whose data would shadow an
    /// envelope or row field.
    #[test]
    fn a_single_row_renders_without_the_results_wrapper() {
        let single = json!({"base":"/r","results":[{"index":0,"status":"empty",
            "data":{"files":[{"path":"a.rs","line":3}],"hints":["widen"]}}]});
        let text = render_tool(ToolId::AstSearch, &single, &json!({}), TextFormat::Yaml);
        assert_eq!(
            text,
            "base: /r\nstatus: empty\nfiles:\n- path: a.rs\n  line: 3\nhints:\n- widen\n"
        );
        let batch = json!({"results":[{"index":0,"data":{"a":1}},{"index":1,"data":{"a":2}}]});
        let text = render_tool(ToolId::AstSearch, &batch, &json!({}), TextFormat::Yaml);
        assert!(text.starts_with("results:\n- index: 0\n"), "{text}");
        let shadowing = json!({"results":[{"index":0,"status":"error","data":{"status":"open"}}]});
        let text = render_tool(ToolId::AstSearch, &shadowing, &json!({}), TextFormat::Yaml);
        assert!(text.starts_with("results:\n- index: 0\n"), "{text}");
        // JSON text stays the exact structured envelope.
        let json_text = render_tool(ToolId::AstSearch, &single, &json!({}), TextFormat::Json);
        assert_eq!(
            serde_json::from_str::<Value>(&json_text).expect("json"),
            single
        );
    }

    /// Path-list rows (`resultView:"files"`) and count rows render as one
    /// string each: `path`, or `path (count)` when the row carries one count.
    #[test]
    fn search_path_rows_render_as_compact_strings() {
        let response = json!({"results":[{"index":0,"data":{"files":[
            {"path":"a.rs"},
            {"path":"b.rs","totalOccurrences":3},
            {"path":"c.rs","totalMatchedLines":2},
            {"path":"d.rs","matches":[{"line":1,"value":"x"}]}]}}]});
        let text = render_tool(ToolId::LocalSearch, &response, &json!({}), TextFormat::Yaml);
        assert!(
            text.starts_with("files:\n- a.rs\n- b.rs (3)\n- c.rs (2)\n- path: d.rs\n"),
            "{text}"
        );
        let json_text = render_tool(ToolId::LocalSearch, &response, &json!({}), TextFormat::Json);
        assert!(json_text.contains(r#"{"path":"a.rs"}"#), "{json_text}");
    }
}
