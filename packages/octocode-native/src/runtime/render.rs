use serde_json::{Value, json};

pub fn render_tool(tool: &str, response: &Value, query: &Value) -> String {
    if tool == "localFetch" {
        return render_local_fetch(response);
    }
    let mut response = response.clone();
    if tool == "localSearch" {
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
                                "rankingProfile",
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
    }
    if tool == "ghGetFileContent" {
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
                            "resolvedBranch",
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
                                    "limit",
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
        return yaml(
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
    }
    yaml(
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

pub fn mcp_result(structured: Value) -> Value {
    let text = render_local_fetch(&structured);
    let rows = structured["results"].as_array();
    let is_error = rows
        .is_some_and(|rows| !rows.is_empty() && rows.iter().all(|row| row["status"] == "error"));
    json!({"content":[{"type":"text","text":text}],"structuredContent":structured,"isError":is_error})
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
    for range in ranges {
        let start = range["start"].as_u64()?;
        let end = range["end"].as_u64()?;
        if start < 1 || end < start || end - start >= records.len() as u64 {
            return None;
        }
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
}
