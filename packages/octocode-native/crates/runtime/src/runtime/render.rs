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
            Self::Yaml => yaml(value, keys),
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
            ToolId::GhSearchRepo
            | ToolId::GhSearchCode
            | ToolId::GhStructure
            | ToolId::GhSearchHistory
            | ToolId::GhCloneRepo
            | ToolId::ArtifactSearch
            | ToolId::StructureSearch
            | ToolId::AstSearch
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
    format.encode(
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
    )
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
        text.push_str(&format!("\n=== patch {header} ===\n{patch}\n"));
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

fn source_lines(content: &str, value: &Value) -> Option<String> {
    let ranges = value.as_array().filter(|v| !v.is_empty())?;
    let records: Vec<&str> = content.split_inclusive('\n').collect();
    let mut output = String::new();
    let mut index = 0;
    let mut prev_end: Option<u64> = None;
    for range in ranges {
        let start = range["start"].as_u64()?;
        let end = range["end"].as_u64()?;
        if start < 1 || end < start || end - start >= records.len() as u64 {
            return None;
        }
        // Non-adjacent windows are separated by one unnumbered omission marker.
        if prev_end.is_some_and(|prev| start > prev + 1)
            && records
                .get(index)
                .is_some_and(|r| r.starts_with("... [line"))
        {
            output.push_str(records[index]);
            index += 1;
        }
        prev_end = Some(end);
        for line in start..=end {
            output.push_str(&format!("{line}: {}", records.get(index)?));
            index += 1;
        }
    }
    (index == records.len()).then_some(output)
}

pub fn render_local_fetch(response: &Value) -> String {
    let mut lines = Vec::new();
    if let Some(base) = response["base"].as_str() {
        lines.extend([format!("base: {base}"), String::new()]);
    }
    for row in response["results"].as_array().into_iter().flatten() {
        let data = &row["data"];
        let mut metadata = data.clone();
        if let Some(map) = metadata.as_object_mut() {
            map.remove("content");
        }
        order_read_metadata(&mut metadata);
        let status = row["status"]
            .as_str()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        lines.push(format!("result: {}{status}", row["index"]));
        let formatted = yaml(
            json!({"data":metadata}),
            &[
                "data",
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
        if !formatted.trim_end().is_empty() {
            lines.push(formatted.trim_end().into());
        }
        if let Some(content) = data["content"].as_str() {
            match source_lines(content, &data["sourceLineRanges"]) {
                Some(numbered) => {
                    lines.push("content (source lines):".into());
                    lines.push(numbered);
                }
                None => {
                    lines.push("content (copy-safe):".into());
                    lines.push(content.into());
                }
            }
        }
        lines.push(String::new());
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
    fn source_numbering_rejects_incomplete_or_invalid_mapping() {
        assert_eq!(
            source_lines("a\nb\n", &json!([{"start":4,"end":5}])),
            Some("4: a\n5: b\n".into())
        );
        assert_eq!(source_lines("a\nb\n", &json!([{"start":4,"end":4}])), None);
        assert_eq!(source_lines("a\n", &json!([{"start":0,"end":1}])), None);
    }

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
                 \n=== patch R +2 -0 src/b.ts <- old/b.ts (2 of 900 chars) ===\n+b\n\
                 \n=== patch A +9 -0 src/c.ts (chars 10-12 of 30) ===\n+c\n",
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

    #[test]
    fn source_lines_passes_omission_marker_through_unnumbered() {
        assert_eq!(
            source_lines(
                "a\n... [lines 3-8 omitted] ...\nb\n",
                &json!([{"start":2,"end":2},{"start":9,"end":9}])
            ),
            Some("2: a\n... [lines 3-8 omitted] ...\n9: b\n".into())
        );
    }
}
