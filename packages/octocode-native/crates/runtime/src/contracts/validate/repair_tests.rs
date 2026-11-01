//! Each likely misuse shape an agent sends gets one error line that says
//! how to send it instead. The cases are the shapes seen in the field and
//! their near relatives; each asserts the whole first line, so a vaguer
//! message fails here.
use crate::contracts::{format_input_error, prepare_rows};
use serde_json::{Value, json};

/// The first error line `tool` answers `input` with.
fn first_line(tool: &str, input: Value) -> String {
    let rows = match prepare_rows(tool, input) {
        Err(error) => {
            return format_input_error(tool, &error)["error"]
                .as_str()
                .unwrap()
                .into();
        }
        Ok(rows) => rows,
    };
    let error = rows
        .into_iter()
        .find_map(Result::err)
        .expect("the input is rejected");
    format_input_error(tool, &error)["error"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn every_misuse_shape_gets_a_one_line_repair() {
    let cases = [
        (
            "localSearch",
            json!({"queries":[{"path":".","matchString":"x"}]}),
            r#"queries.0.matchString: Expected a JSON array; send ["x"], not "x""#,
        ),
        (
            "ghSearchCode",
            json!({"queries":[{"owner":"o","keywords":"retry"}]}),
            r#"queries.0.keywords: Expected a JSON array; send ["retry"], not "retry""#,
        ),
        (
            "localSearch",
            json!({"queries":[{"path":".","matchString":"[\"x\"]"}]}),
            "queries.0.matchString: Expected a JSON array; send the JSON value itself, not a string holding it",
        ),
        (
            "localSearch",
            json!({"path":".","matchString":["x"]}),
            r#"matchString, path are query fields: send {"queries":[{…}]}, one object per query."#,
        ),
        (
            "localFetch",
            json!({"query":{"path":"README.md"}}),
            r#"Unknown request field 'query': fresh calls send {"queries":[...]}; continuations send {"cursor":"..."} alone."#,
        ),
        (
            "localFetch",
            json!({"queries":{"path":"README.md"}}),
            r#"queries: queries must be an array of query objects; send "queries":[{…}], not "queries":{…}"#,
        ),
        (
            "localFetch",
            json!({"queries":"[{\"path\":\"README.md\"}]"}),
            "queries: queries must be an array of query objects; send the JSON value itself, not a string holding it",
        ),
        (
            "localSearch",
            json!({"queries":[{"path":".","match_string":["x"]}]}),
            "Unknown field 'match_string' in queries[0]; use 'matchString'.",
        ),
        (
            "localSearch",
            json!({"queries":[{"path":".","matchstring":["x"]}]}),
            "Unknown field 'matchstring' in queries[0]; use 'matchString'.",
        ),
        (
            "localSearch",
            json!({"queries":[{"path":".","matchString":["x"],"view":"file"}]}),
            r#"queries.0.view: Value "file" is outside the allowed enum; use "files"; allowed: content, files, filesWithout, countLines, countMatches, matchOnly"#,
        ),
        (
            "localFetch",
            json!({"queries":[{"path":["a.ts","b.ts"]}]}),
            "queries.0.path: Expected string, not an array; send one value, and each other value in its own query row",
        ),
        (
            "localFetch",
            json!({"queries":[{"path":5}]}),
            r#"queries.0.path: Expected string; send "5", not 5"#,
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a.ts","ranges":"1-2"}]}),
            "queries.0.ranges: Expected a JSON array of {endLine, startLine}",
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a.ts","matchString":["x"],"contextLines":3}]}),
            r#"queries.0.contextLines: Expected object {after, before}; send {"before":3,"after":3}, not 3"#,
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a"},{"path":"a"},{"path":"a"},{"path":"a"},{"path":"a"},{"path":"a"}]}),
            "queries: Value length 6 exceeds the maximum of 5; send the other rows in another call",
        ),
        (
            "localSearch",
            json!({"queries":[{"cursor":"issued"}]}),
            r#"'cursor' is a whole request, not a query field: send {"cursor":"…"} alone, as next.input gives it, without queries."#,
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a.ts","responseChars":100}]}),
            "'responseChars' is a request field: put it beside queries, not in queries[0].",
        ),
        (
            "localSearch",
            json!({"queries":[{"path":".","matchString":["x"],"page":2}]}),
            "queries.0.page: page is private progress; copy the issued cursor from next.*.",
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a.ts","snapshot":"s"}]}),
            "queries.0.snapshot: snapshot is private progress; copy the issued cursor from next.*.",
        ),
        (
            "lspSearch",
            json!({"queries":[{"operation":"definition","path":"a.ts","symbolName":"x","line":1,"symbolQuery":"y"}]}),
            r#"'symbolQuery' is not allowed in this form of queries[0]; it requires operation:"workspaceSymbol"."#,
        ),
        (
            // A form-specific field names the selector that admits it, not the
            // field's own value in every form that also pins it.
            "lspSearch",
            json!({"queries":[{"operation":"callers","path":"a.rs","symbolName":"f","line":1,"view":"files"}]}),
            r#"'view' is not allowed in this form of queries[0]; it requires operation one of "definition", "typeDefinition", "implementation", "references"."#,
        ),
        (
            "localFetch",
            json!({"cursor":"issued","queries":[{"path":"a.ts"}]}),
            "`queries` and `cursor` belong to different forms and cannot be combined; send one form",
        ),
        (
            "localFetch",
            json!({"queries":[{"path":"a.ts","ranges":[{"startLine":"1","endLine":2}]}]}),
            r#"queries.0.ranges.0.startLine: Expected number; send 1, not "1""#,
        ),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .filter_map(|(tool, input, expected)| {
            let actual = first_line(tool, input.clone());
            (actual != expected)
                .then(|| format!("{tool} {input}\n  want: {expected}\n  got:  {actual}"))
        })
        .collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn a_far_unknown_field_lists_the_allowed_fields_instead_of_guessing() {
    let line = first_line(
        "localFetch",
        json!({"queries":[{"path":"a.ts","somethingElse":true}]}),
    );
    assert!(
        line.starts_with("Unknown field 'somethingElse' in queries[0]. Allowed fields: "),
        "{line}"
    );
}
