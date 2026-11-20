//! Each likely misuse shape an agent sends gets one error line that says
//! how to send it instead. The cases are the shapes seen in the field and
//! their near relatives; each asserts the whole first line, so a vaguer
//! message fails here.
use crate::contracts::{ContractValidationError, format_input_error, prepare_rows};
use octocode_test_support::json_fixture;
use serde_json::{Value, json};

/// The first error line `tool` answers `input` with.
fn first_line(tool: &str, input: Value) -> String {
    let rows = match prepare_rows(tool, input) {
        Err(error) => {
            return format_input_error(tool, &error, &[], &serde_json::Value::Null)["error"]
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
    format_input_error(tool, &error, &[], &serde_json::Value::Null)["error"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn every_misuse_shape_gets_a_one_line_repair() {
    let cases = [
        (
            "localSearch",
            json_fixture(r#"{"queries": [{"path": ".", "matchString": "x"}]}"#),
            r#"queries.0.matchString: Expected a JSON array; send ["x"], not "x""#,
        ),
        (
            "ghSearchCode",
            json_fixture(r#"{"queries": [{"owner": "o", "keywords": "retry"}]}"#),
            r#"queries.0.keywords: Expected a JSON array; send ["retry"], not "retry""#,
        ),
        (
            "localSearch",
            json_fixture(r#"{"queries": [{"path": ".", "matchString": "[\"x\"]"}]}"#),
            "queries.0.matchString: Expected a JSON array; send the JSON value itself, not a string holding it",
        ),
        (
            "localFetch",
            json_fixture(r#"{"query": {"path": "README.md"}}"#),
            r#"Unknown request field 'query': fresh calls send {"queries":[...]}; continuations send {"cursor":"..."} alone."#,
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": 5}"#),
            r#"queries: queries must be an array of query objects; send "queries":[{…}], not "queries":5"#,
        ),
        (
            "localSearch",
            json_fixture(r#"{"queries": [{"path": ".", "match_string": ["x"]}]}"#),
            "Unknown field 'match_string' in queries[0]; use 'matchString'.",
        ),
        (
            "localSearch",
            json_fixture(r#"{"queries": [{"path": ".", "matchstring": ["x"]}]}"#),
            "Unknown field 'matchstring' in queries[0]; use 'matchString'.",
        ),
        // A former name inside a nested object names its field there.
        (
            "ghSearchRepo",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "search",
                            "keywords": ["x"],
                            "filters": {"forkCount": {"min": 5}}
                        }
                    ]
                }"#,
            ),
            "Unknown field 'forkCount' in queries[0].filters; use 'forks'.",
        ),
        (
            "ghSearchHistory",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "issue",
                            "owner": "o",
                            "repo": "r",
                            "keywords": ["x"],
                            "filters": {"commentCount": {"min": 1}}
                        }
                    ]
                }"#,
            ),
            "Unknown field 'commentCount' in queries[0].filters; use 'commentsCount'.",
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [{"path": ".", "matchString": ["x"], "view": "file"}]
                }"#,
            ),
            r#"queries.0.view: Value "file" is outside the allowed enum; use "files"; allowed: content, files, filesWithout, countLines, countMatches, matchOnly"#,
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": ["a.ts", "b.ts"]}]}"#),
            "queries.0.path: Expected string, not an array; send one value, and each other value in its own query row",
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": 5}]}"#),
            r#"queries.0.path: Expected string; send "5", not 5"#,
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": "a.ts", "ranges": "1-2"}]}"#),
            "queries.0.ranges: Expected a JSON array of {endLine, startLine}",
        ),
        (
            "localFetch",
            json_fixture(
                r#"{
                    "queries": [{"path": "a.ts", "matchString": ["x"], "contextLines": 3}]
                }"#,
            ),
            r#"queries.0.contextLines: Expected object {after, before}; send {"before":3,"after":3}, not 3"#,
        ),
        (
            "localFetch",
            json_fixture(
                r#"{
                    "queries": [
                        {"path": "a"},
                        {"path": "a"},
                        {"path": "a"},
                        {"path": "a"},
                        {"path": "a"},
                        {"path": "a"}
                    ]
                }"#,
            ),
            "queries: Value length 6 exceeds the maximum of 5; send the other rows in another call",
        ),
        (
            "localSearch",
            json_fixture(r#"{"queries": [{"cursor": "issued"}]}"#),
            r#"'cursor' is a whole request, not a query field: send {"cursor":"…"} alone, as next.nextPage or responsePagination.next gives it, without queries."#,
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": "a.ts", "responseChars": 100}]}"#),
            "'responseChars' is a request field: put it beside queries, not in queries[0].",
        ),
        (
            "localSearch",
            json_fixture(
                r#"{
                    "queries": [{"path": ".", "matchString": ["x"], "page": 2}]
                }"#,
            ),
            "queries.0.page: page is private progress; copy the issued cursor from next.*.",
        ),
        (
            "localFetch",
            json_fixture(r#"{"queries": [{"path": "a.ts", "snapshot": "s"}]}"#),
            "queries.0.snapshot: snapshot is private progress; copy the issued cursor from next.*.",
        ),
        (
            "lspSearch",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "definition",
                            "path": "a.ts",
                            "symbolName": "x",
                            "line": 1,
                            "symbolFuzzy": "y"
                        }
                    ]
                }"#,
            ),
            r#"'symbolFuzzy' is not allowed in this form of queries[0]; it requires operation:"workspaceSymbol"."#,
        ),
        (
            // A form-specific field names the selector that admits it, not the
            // field's own value in every form that also pins it.
            "lspSearch",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "callers",
                            "path": "a.rs",
                            "symbolName": "f",
                            "line": 1,
                            "view": "files"
                        }
                    ]
                }"#,
            ),
            r#"'view' is not allowed in this form of queries[0]; it requires operation one of "definition", "typeDefinition", "implementation", "references"."#,
        ),
        (
            "localFetch",
            json_fixture(r#"{"cursor": "issued", "queries": [{"path": "a.ts"}]}"#),
            r#"A cursor call takes the cursor alone: send {"cursor":"…"} as next.nextPage or responsePagination.next gives it, without 'queries'; a new request sends 'queries' without cursor."#,
        ),
        (
            // A row that is wrong beside the cursor: the cursor rule leads.
            "ghSearchRepo",
            json_fixture(
                r#"{
                    "cursor": "issued",
                    "queries": [{"sort": {"by": "stars", "order": "desc"}}]
                }"#,
            ),
            r#"A cursor call takes the cursor alone: send {"cursor":"…"} as next.nextPage or responsePagination.next gives it, without 'queries'; a new request sends 'queries' without cursor."#,
        ),
        (
            // A former sort value names the value it is now, in any spelling.
            "ghSearchRepo",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "search",
                            "keywords": ["x"],
                            "sort": {"by": "best-match", "order": "desc"}
                        }
                    ]
                }"#,
            ),
            r#"queries.0.sort.by: Value "best-match" is outside the allowed enum; use "relevance"; allowed: relevance, stars, forks, helpWantedIssueCount, updatedAt"#,
        ),
        (
            "ghSearchRepo",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "search",
                            "keywords": ["x"],
                            "sort": {"by": "forkCount", "order": "desc"}
                        }
                    ]
                }"#,
            ),
            r#"queries.0.sort.by: Value "forkCount" is outside the allowed enum; use "forks"; allowed: relevance, stars, forks, helpWantedIssueCount, updatedAt"#,
        ),
        (
            // A cursor keeps the page size it was issued with.
            "localSearch",
            json_fixture(r#"{"cursor": "issued", "responseChars": 900}"#),
            r#"'responseChars' is not allowed beside cursor: send {"cursor":"…"} alone, as next.nextPage or responsePagination.next gives it; 'responseChars' requires queries, in a new request."#,
        ),
        (
            "localFetch",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "path": "a.ts",
                            "ranges": [{"startLine": "1", "endLine": 2}]
                        }
                    ]
                }"#,
            ),
            r#"queries.0.ranges.0.startLine: Expected number; send 1, not "1""#,
        ),
        (
            // A form that needs the row's own ecosystem changed as well as its
            // operation adds nothing to the form that needs the operation only.
            "artifactSearch",
            json_fixture(
                r#"{
                    "queries": [
                        {
                            "operation": "resolve",
                            "ecosystem": "npm",
                            "packageName": "express",
                            "keywords": ["x"]
                        }
                    ]
                }"#,
            ),
            r#"'keywords' is not allowed in this form of queries[0]; it requires operation:"search"."#,
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
        json_fixture(r#"{"queries": [{"path": "a.ts", "somethingElse": true}]}"#),
    );
    assert!(
        line.starts_with("Unknown field 'somethingElse' in queries[0]. Allowed fields: "),
        "{line}"
    );
}

/// A near-miss ecosystem names the one value it means, from the contract's own
/// enum and purl types: no alias is written here.
#[test]
fn a_near_miss_ecosystem_names_the_one_it_means() {
    let wrong = |ecosystem: &str| {
        first_line(
            "securityCheck",
            json!({"queries":[{"operation":"package","ecosystem":ecosystem,"packageName":"time"}]}),
        )
    };
    for (sent, meant) in [
        ("NPM", "npm"),
        ("crates", "crates.io"),
        ("cargo", "crates.io"),
        ("golang", "Go"),
        ("gem", "RubyGems"),
        ("composer", "Packagist"),
        ("rocky", "Rocky Linux"),
    ] {
        let line = wrong(sent);
        assert!(line.contains(&format!("use \"{meant}\"")), "{sent}: {line}");
    }
}

/// `deb`, `apk` and `rpm` are purl types, not typos of another ecosystem: each
/// names every ecosystem the contract's purl table lists for it, never `npm`.
#[test]
fn a_distribution_purl_type_names_each_ecosystem_the_contract_lists() {
    let contract = crate::contracts::security::security_contract().expect("security block");
    for purl_type in ["deb", "apk", "rpm"] {
        let ecosystems = &contract.ecosystems.purl_types[purl_type];
        assert!(ecosystems.len() > 1, "{purl_type}");
        let line = first_line(
            "securityCheck",
            json!({"queries":[{"operation":"package","ecosystem":purl_type,"packageName":"bash"}]}),
        );
        let head = ecosystems[..ecosystems.len() - 1]
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let want = format!("use {head} or \"{}\"; ", ecosystems.last().unwrap());
        assert!(line.contains(&want), "{purl_type}: {line}");
    }
}

/// An astSearch rule sent as a string, or holding only a key rules do not
/// take, reads the error of what was sent: the type, or the unknown key with
/// the keys a rule takes. Neither reads a relation or matcher rule of a
/// shape it never had.
#[test]
fn a_misshapen_ast_rule_names_its_own_fault() {
    let row = |rule: Value| json!({"queries":[{"operation":"match","path":".","rule":rule}]});
    let string = first_line("astSearch", row(json!("errorMessage($E)")));
    assert_eq!(
        string,
        r#"queries.0.rule: Expected object {all, any, field, follows, has, inside, kind, not, nthChild, pattern, precedes, range, regex, stopBy}; "errorMessage($E)" is the value of one field: send {"<field>":"errorMessage($E)"} with <field> one of field, kind, pattern, regex"#
    );
    let allowed = "Allowed fields: all, any, field, follows, has, inside, kind, not, nthChild, pattern, precedes, range, regex, stopBy.";
    for rule in [
        json_fixture(r#"{"contains": {"kind": "identifier"}}"#),
        json_fixture(
            r#"{
                "kind": "call_expression",
                "contains": {"kind": "identifier"}
            }"#,
        ),
    ] {
        assert_eq!(
            first_line("astSearch", row(rule)),
            format!("Unknown field 'contains' in queries[0].rule. {allowed}")
        );
    }
    // A rule of the right shape without a matcher still reads its guidance.
    assert_eq!(
        first_line("astSearch", row(json!({}))),
        "queries.0.rule: rule must contain at least one matcher"
    );
}

/// A validation error shown as text states its issues: a caller that embeds
/// it (a tool validating a derived query) passes on the correction, never a
/// count of issues.
#[test]
fn a_validation_error_shown_as_text_states_its_issues() {
    let error = crate::contracts::validate_query(
        "artifactSearch",
        json_fixture(
            r#"{
                "operation": "resolve",
                "ecosystem": "npm",
                "packageName": "ws@8.5.0"
            }"#,
        ),
    )
    .expect_err("a version in the name is refused");
    let shown = error.to_string();
    assert!(
        shown.contains("packageName") && shown.contains("version is separate"),
        "{shown}"
    );
    assert!(!shown.contains("issue(s)"), "{shown}");
    let two = ContractValidationError {
        issues: vec![
            super::issue("probe.a", vec!["a".into(), "0".into()], "first")
                .issues
                .remove(0),
            super::issue("probe.b", vec![], "second").issues.remove(0),
        ],
    };
    assert_eq!(two.to_string(), "a.0: first; second");
}

/// Every tool's rejected call ends with what repairs it when the cause is
/// not enough: one request the tool accepts, once. The command that prints
/// the schema is the CLI's to add (an MCP client holds the schema already),
/// so the transport-neutral answer names none. The answer stays its causes
/// and that line, never the schema itself.
#[test]
fn a_rejected_call_names_an_accepted_example() {
    for tool in crate::id::ToolId::ALL {
        let tool = tool.as_str();
        let error = prepare_rows(tool, json_fixture(r#"{"queries": [{"noSuchField": 1}]}"#))
            .map(|rows| {
                rows.into_iter()
                    .find_map(Result::err)
                    .unwrap_or_else(|| panic!("{tool} rejects an unknown field"))
            })
            .unwrap_or_else(|error| error);
        let envelope = format_input_error(tool, &error, &[], &serde_json::Value::Null);
        let details = envelope["details"].as_array().expect("details");
        let lines = details.iter().filter_map(Value::as_str).collect::<Vec<_>>();
        let examples = lines
            .iter()
            .filter_map(|line| line.strip_prefix("Example: "))
            .collect::<Vec<_>>();
        let [example] = examples.as_slice() else {
            panic!("{tool}: one example, got {lines:?}");
        };
        let example: Value = serde_json::from_str(example).expect("the example is a request");
        crate::contracts::validate(tool, example.clone())
            .unwrap_or_else(|error| panic!("{tool}: its example {example} is rejected: {error}"));
        assert!(
            lines
                .last()
                .is_some_and(|line| line.starts_with("Example: ")),
            "{tool}: {lines:?}"
        );
        assert!(
            !envelope.to_string().contains("octocode schema"),
            "{tool}: no CLI command in a transport-neutral answer: {envelope}"
        );
        assert!(
            !envelope["error"]
                .as_str()
                .expect("cause")
                .starts_with("Example"),
            "{tool}: the cause leads: {envelope}"
        );
        assert!(
            !envelope.to_string().contains("\"properties\""),
            "{tool}: the answer is not the schema: {envelope}"
        );
    }
    // A name that is no tool has no example to offer.
    let unknown = format_input_error(
        "noSuchTool",
        &ContractValidationError { issues: vec![] },
        &[],
        &serde_json::Value::Null,
    );
    assert!(unknown.get("details").is_none(), "{unknown}");
}

/// The example beside a rejected row is of the operation that row sent, not
/// of the tool's first form: a `compare` row with a wrong section is shown a
/// `compare` call.
#[test]
fn the_example_is_of_the_operation_the_row_sent() {
    let sent = json_fixture(
        r#"{
            "queries": [
                {
                    "operation": "compare",
                    "owner": "o",
                    "repo": "r",
                    "baseRef": "v1",
                    "headRef": "v2",
                    "sections": ["bogus"]
                }
            ]
        }"#,
    );
    let error = prepare_rows("ghGetHistoryItem", sent.clone())
        .map(|rows| rows.into_iter().find_map(Result::err).expect("rejected"))
        .unwrap_or_else(|error| error);
    let example = |sent: &Value| {
        format_input_error("ghGetHistoryItem", &error, &[], sent)["details"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .find_map(|line| line.strip_prefix("Example: "))
            .map(|example| serde_json::from_str::<Value>(example).expect("request"))
            .expect("an example")
    };
    assert_eq!(example(&sent)["queries"][0]["operation"], "compare");
    // Nothing sent to read: the first form's.
    assert_eq!(
        example(&Value::Null)["queries"][0]["operation"],
        "pullRequest"
    );
}

/// A value the input guard refused sits in a well-formed call: the cause
/// names the whole repair, so no unrelated example or schema pointer
/// follows it. Beside a shape issue the reference still comes.
#[test]
fn a_refused_value_is_its_own_repair_without_an_example() {
    let refused = super::issue(
        "security.credential",
        vec!["queries".into(), "0".into(), "matchString[]".into()],
        "Value looks like a credential and was not run",
    );
    let envelope = format_input_error("localSearch", &refused, &[], &serde_json::Value::Null);
    assert!(envelope.get("details").is_none(), "{envelope}");

    let mut mixed = refused;
    mixed.issues.push(
        super::issue(
            "schema.type",
            vec!["queries".into(), "0".into(), "path".into()],
            "Expected a string",
        )
        .issues
        .remove(0),
    );
    let envelope = format_input_error("localSearch", &mixed, &[], &serde_json::Value::Null);
    assert!(
        envelope["details"]
            .as_array()
            .is_some_and(|lines| lines.iter().any(|line| line
                .as_str()
                .is_some_and(|line| line.starts_with("Example: ")))),
        "{envelope}"
    );
}

/// The example beside a rejected row is of a form that takes every field the
/// row sent, among the forms of the operation it named: each form's own
/// example, sent with one unknown field, is shown an accepted request of a
/// form that takes all of its fields (a `source` row a `source` call, a
/// commit `path` row a commit `path` call), never just the operation's first
/// form.
#[test]
fn the_example_is_of_a_form_taking_every_field_the_row_sent() {
    let presentation = crate::contracts::cli_presentation().expect("presentation");
    let mut checked = 0;
    for entry in presentation["tools"].as_array().expect("tools") {
        let tool = entry["name"].as_str().expect("name");
        let variants = entry["views"]["variants"]["variants"]
            .as_array()
            .expect("variants");
        for variant in variants {
            let Some(example) = variant["example"].as_object() else {
                continue;
            };
            let mut row = example.clone();
            row.insert("noSuchField".into(), json!(1));
            let sent = json!({"queries": [row]});
            let error = match prepare_rows(tool, sent.clone()) {
                Err(error) => error,
                Ok(rows) => rows.into_iter().find_map(Result::err).expect("rejected"),
            };
            let shown = format_input_error(tool, &error, &[], &sent)["details"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find_map(|line| line.strip_prefix("Example: "))
                .map(|line| serde_json::from_str::<Value>(line).expect("request"))
                .unwrap_or_else(|| panic!("{tool}: an example"));
            crate::contracts::validate(tool, shown.clone())
                .unwrap_or_else(|error| panic!("{tool}: {shown} is rejected: {error}"));
            let shown_fields = shown["queries"][0].as_object().expect("row");
            let takes_all = variants.iter().any(|form| {
                let takes = |field: &String| {
                    form["fields"]
                        .as_array()
                        .is_some_and(|fields| fields.iter().any(|known| known == field))
                };
                example.keys().all(takes) && shown_fields.keys().all(takes)
            });
            assert!(takes_all, "{tool}: {sent} was shown {shown}");
            checked += 1;
        }
    }
    assert!(checked > 0);
}

/// A tie between nearest names is no one guess: the error names the tied
/// fields only, not every field the object allows.
#[test]
fn a_tie_names_only_the_tied_fields() {
    let line = first_line(
        "ghSearchHistory",
        json_fixture(
            r#"{
                "queries": [
                    {
                        "operation": "pullRequest",
                        "owner": "o",
                        "repo": "r",
                        "filters": {"merged_": true}
                    }
                ]
            }"#,
        ),
    );
    assert_eq!(
        line,
        "Unknown field 'merged_' in queries[0].filters; use 'isMerged' or 'mergedAt'.",
    );
}

/// A field this tool lacks that a tool its description names declares is
/// that tool's: the error says which tool takes it.
#[test]
fn a_field_of_a_named_tool_says_which_tool_takes_it() {
    for (tool, row, owner) in [
        (
            "graphBuild",
            json_fixture(r#"{"path": ".", "operation": "stats"}"#),
            "graphQuery",
        ),
        (
            "graphQuery",
            json_fixture(r#"{"operation": "stats", "keep": 2}"#),
            "graphBuild",
        ),
    ] {
        let line = first_line(tool, json!({"queries":[row]}));
        assert!(
            line.contains(&format!("is a {owner} field")),
            "{tool}: {line}"
        );
    }
}

/// The envelope `tool` answers `sent` with, read as the runtime reads it
/// (repaired, then validated), and its error and detail lines.
fn answered(tool: &str, sent: &Value) -> (Value, Vec<String>) {
    let sent = &crate::contracts::normalized_input(tool, sent.clone()).0;
    let error = match prepare_rows(tool, sent.clone()) {
        Err(error) => error,
        Ok(rows) => rows
            .into_iter()
            .find_map(Result::err)
            .unwrap_or_else(|| panic!("{tool} rejects {sent}")),
    };
    let envelope = format_input_error(tool, &error, &[], sent);
    let lines = std::iter::once(&envelope["error"])
        .chain(envelope["details"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    (envelope, lines)
}

/// The row of the example `tool` shows beside the rejected `row`, checked to
/// be a request the tool accepts.
fn shown_example(tool: &str, row: Value) -> Value {
    let sent = json!({"queries": [row]});
    let (_, lines) = answered(tool, &sent);
    let shown = lines
        .iter()
        .find_map(|line| line.strip_prefix("Example: "))
        .map(|line| serde_json::from_str::<Value>(line).expect("request"))
        .unwrap_or_else(|| panic!("{tool}: an example in {lines:?}"));
    crate::contracts::validate(tool, shown.clone())
        .unwrap_or_else(|error| panic!("{tool}: {shown} is rejected: {error}"));
    shown["queries"][0].clone()
}

/// The example beside a rejected row keeps the row's own value of each
/// field it shows that the row sent without an issue: a graphBuild of a
/// package with a misspelt field is shown a graphBuild of that package, not
/// of the whole workspace. A field at fault shows the example's value, and
/// an example field the row did not send is shown only where the request
/// needs it. The example stays a request the tool accepts.
#[test]
fn the_example_keeps_the_rows_own_valid_values() {
    assert_eq!(
        shown_example(
            "graphBuild",
            json_fixture(r#"{"path": "packages/app", "refreshh": true}"#)
        ),
        json_fixture(r#"{"path": "packages/app"}"#)
    );
    assert_eq!(
        shown_example(
            "localFetch",
            json_fixture(
                r#"{"path": "a.rs", "ranges": [{"startLine": 3, "endLine": 4}], "bogus": 1}"#
            )
        ),
        json_fixture(r#"{"path": "a.rs", "ranges": [{"startLine": 3, "endLine": 4}]}"#)
    );
    let shown = shown_example(
        "securityCheck",
        json_fixture(r#"{"operation": "package", "ecosystem": "nmp", "packageName": "left-pad"}"#),
    );
    assert_eq!(
        (&shown["operation"], &shown["packageName"]),
        (&json!("package"), &json!("left-pad")),
        "{shown}"
    );
    assert!(
        shown["ecosystem"].is_string() && shown["ecosystem"] != "nmp",
        "{shown}"
    );
    assert!(shown.get("version").is_none(), "{shown}");
}

/// A row at fault as a whole (two forms mixed) names no field at fault: the
/// example still takes the row's own values wherever the request stays
/// valid, so it searches the caller's repository, never core's example
/// one, and keeps one of the mixed forms. A field-level fault keeps the
/// rule above: the field at fault shows the example's value.
#[test]
fn a_row_level_fault_is_shown_the_rows_own_repository() {
    let mixed = json_fixture(
        r#"{"operation": "commit", "owner": "tokio-rs", "repo": "axum",
            "keywords": ["router"], "path": "axum/src"}"#,
    );
    let shown = shown_example("ghSearchHistory", mixed);
    assert_eq!(
        (&shown["owner"], &shown["repo"], &shown["operation"]),
        (&json!("tokio-rs"), &json!("axum"), &json!("commit")),
        "{shown}"
    );
    assert!(
        shown.get("keywords").is_some() != shown.get("path").is_some(),
        "one form: {shown}"
    );
    let field_level = json_fixture(
        r#"{"operation": "commit", "owner": "tokio-rs", "repo": "axum",
            "keywords": ["router"], "bogus": 1}"#,
    );
    assert_eq!(
        shown_example("ghSearchHistory", field_level),
        json_fixture(
            r#"{"operation": "commit", "owner": "tokio-rs", "repo": "axum",
                "keywords": ["router"]}"#
        )
    );
}

/// Forms that take the row's fields alike are told apart by the row's
/// values: a Maven row, whether its coordinate or another field is at
/// fault, is shown a Maven example, never the first form's npm one, and
/// the example is a request the tool accepts.
#[test]
fn a_tie_between_forms_picks_the_one_sharing_the_rows_values() {
    for row in [
        json!({"operation": "resolve", "ecosystem": "maven", "packageName": "log4j-core"}),
        json!({"operation": "resolve", "ecosystem": "maven",
            "packageName": "org.apache.logging.log4j:log4j-core", "noSuchField": 1}),
    ] {
        let shown = shown_example("artifactSearch", row.clone());
        assert_eq!(shown["ecosystem"], row["ecosystem"], "{shown} beside {row}");
    }
}

/// A required value the row lacks is shown as a visible placeholder (its
/// name in capitals), never core's example value, which reads as a real
/// name: an owner-only repository `get` is shown that owner and `REPO`.
#[test]
fn a_missing_required_value_is_a_placeholder_not_a_real_looking_name() {
    let shown = shown_example(
        "ghSearchRepo",
        json_fixture(r#"{"operation": "get", "owner": "tokio-rs"}"#),
    );
    assert_eq!(shown["owner"], "tokio-rs", "{shown}");
    assert_eq!(
        shown["repo"].as_str(),
        Some("repo".to_ascii_uppercase().as_str()),
        "{shown}"
    );
}

/// A field only one form takes picks that form: a required field the row
/// then lacks is named with the field that picked it (clasify `perHit`
/// sent beside `content` needs `source`).
#[test]
fn a_missing_field_names_the_field_that_picked_its_form() {
    let sent = json_fixture(
        r#"{"queries": [{"operation": "evaluate", "perHit": true, "content": "abc",
            "questions": {"x": {"type": "noul", "instructions": "is it x"}}}]}"#,
    );
    let (_, lines) = answered("clasify", &sent);
    let named = lines
        .iter()
        .filter(|line| line.contains("Missing required field: source") && line.contains("perHit"))
        .count();
    assert_eq!(named, 1, "{lines:?}");
}

/// A fault inside a nested query (an unknown structureSearch field in a
/// clasify `source.query`) leaves the rest of that value the row's own: the
/// example keeps the tool the row names, without the field at fault.
#[test]
fn a_nested_fault_keeps_the_rows_own_nested_value() {
    let shown = shown_example(
        "clasify",
        json_fixture(
            r#"{"operation": "evaluate",
                "questions": {"x": {"type": "noul", "instructions": "is it x"}},
                "source": {"tool": "structureSearch", "query": {"path": ".", "bogusField": 1}}}"#,
        ),
    );
    assert_eq!(
        shown["source"],
        json_fixture(r#"{"tool": "structureSearch", "query": {"path": "."}}"#),
        "{shown}"
    );
}

/// The example also keeps each field the row sent without an issue that
/// core's example of the form leaves out: a graphQuery `hubs` of a package
/// with a misspelt field is shown a `hubs` of that package, and a `deps`
/// keeps its `target` and `path`. The row's brief (`mainGoal`, `reasoning`)
/// describes the call, not its form, and stays out of the example.
#[test]
fn the_example_keeps_the_rows_fields_its_form_example_leaves_out() {
    for (row, shown) in [
        (
            r#"{"operation": "hubs", "path": "packages/app", "bogus": 1}"#,
            r#"{"operation": "hubs", "path": "packages/app"}"#,
        ),
        (
            r#"{"operation": "deps", "target": "core.js", "path": "packages/app", "bogus": 1,
                "mainGoal": "What core.js imports", "reasoning": "deps lists them"}"#,
            r#"{"operation": "deps", "target": "core.js", "path": "packages/app"}"#,
        ),
    ] {
        assert_eq!(
            shown_example("graphQuery", json_fixture(row)),
            json_fixture(shown),
            "{row}"
        );
    }
}

/// A cursor sent inside a query row is a whole request: the error says so,
/// and the fields the row lacks as a query, which a cursor call never
/// sends, are not named. A cursor beside rows is answered by the same rule
/// first, before what the rows lack.
#[test]
fn a_cursor_in_a_row_is_answered_by_the_cursor_alone_rule() {
    let in_row = json_fixture(r#"{"queries": [{"cursor": "cursor2.x"}]}"#);
    let beside = json_fixture(r#"{"queries": [{}], "cursor": "cursor2.x"}"#);
    for tool in ["localFetch", "localSearch", "ghSearchCode"] {
        let (envelope, lines) = answered(tool, &in_row);
        let error = envelope["error"].as_str().expect("error");
        assert!(
            error.contains(r#"{"cursor":"…"}"#) && error.contains(" alone"),
            "{tool}: {envelope}"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("Missing required field")),
            "{tool}: {envelope}"
        );
        let (envelope, _) = answered(tool, &beside);
        let error = envelope["error"].as_str().expect("error");
        assert!(
            error.contains(r#"{"cursor":"…"}"#) && error.contains("cursor alone"),
            "{tool}: {envelope}"
        );
    }
}

/// A cursor beside `queries`, an empty list included, is answered by the
/// cursor-alone rule, said of the request: never of a row, and never with a
/// list of allowed fields that holds the field it refuses.
#[test]
fn a_cursor_beside_queries_is_answered_of_the_request() {
    let sent = json_fixture(r#"{"queries": [], "cursor": "cursor2.x"}"#);
    for tool in ["structureSearch", "localFetch"] {
        let (envelope, lines) = answered(tool, &sent);
        let error = envelope["error"].as_str().expect("error");
        assert!(
            error.contains(r#"{"cursor":"…"}"#)
                && error.contains("cursor alone")
                && error.contains("the request"),
            "{tool}: {envelope}"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("queries[0]") || line.contains("Allowed fields")),
            "{tool}: {envelope}"
        );
    }
}

/// A value outside its enum whose message names the one literal it means
/// (`sort.by: "modified"`, `use "modifiedAt"`) is repaired in the example,
/// not dropped: the example still asks what the row did, here a sorted
/// listing, and runs as shown.
#[test]
fn the_example_takes_the_literal_a_near_miss_means() {
    for (tool, row, field, expected) in [
        (
            "structureSearch",
            json!({"operation": "files", "path": "src", "entryType": "file", "view": "details",
                "sort": {"by": "modified", "order": "desc"}, "limit": 5}),
            "sort",
            json!({"by": "modifiedAt", "order": "desc"}),
        ),
        (
            "localSearch",
            json!({"path": "src", "matchString": ["needle"], "caseMode": "insensitiv"}),
            "caseMode",
            json!("insensitive"),
        ),
    ] {
        let sent = json!({"queries": [row]});
        let error = match prepare_rows(tool, sent.clone()) {
            Err(error) => error,
            Ok(rows) => rows.into_iter().find_map(Result::err).expect("rejected"),
        };
        let envelope = format_input_error(tool, &error, &[], &sent);
        let shown = envelope["details"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .find_map(|line| line.strip_prefix("Example: "))
            .map(|line| serde_json::from_str::<Value>(line).expect("request"))
            .unwrap_or_else(|| panic!("{tool}: an example in {envelope}"));
        assert_eq!(shown["queries"][0][field], expected, "{tool}: {shown}");
        crate::contracts::validate(tool, shown.clone())
            .unwrap_or_else(|error| panic!("{tool}: {shown} is rejected: {error}"));
    }
}
