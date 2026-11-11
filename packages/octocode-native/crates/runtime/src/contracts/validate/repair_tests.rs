//! Each likely misuse shape an agent sends gets one error line that says
//! how to send it instead. The cases are the shapes seen in the field and
//! their near relatives; each asserts the whole first line, so a vaguer
//! message fails here.
use crate::contracts::{ContractValidationError, format_input_error, prepare_rows};
use serde_json::{Value, json};

/// The first error line `tool` answers `input` with.
fn first_line(tool: &str, input: Value) -> String {
    let rows = match prepare_rows(tool, input) {
        Err(error) => {
            return format_input_error(tool, &error, &[])["error"]
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
    format_input_error(tool, &error, &[])["error"]
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
            "localFetch",
            json!({"query":{"path":"README.md"}}),
            r#"Unknown request field 'query': fresh calls send {"queries":[...]}; continuations send {"cursor":"..."} alone."#,
        ),
        (
            "localFetch",
            json!({"queries":5}),
            r#"queries: queries must be an array of query objects; send "queries":[{…}], not "queries":5"#,
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
            // A cursor keeps the page size it was issued with.
            "localSearch",
            json!({"cursor":"issued","responseChars":900}),
            r#"'responseChars' is not allowed beside cursor: send {"cursor":"…"} alone, as next.input gives it; 'responseChars' requires queries, in a new request."#,
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
        json!({"contains":{"kind":"identifier"}}),
        json!({"kind":"call_expression","contains":{"kind":"identifier"}}),
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
        json!({"operation":"resolve","ecosystem":"npm","packageName":"ws@8.5.0"}),
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
/// not enough: one request the tool accepts and the command that prints its
/// schema, each once. The answer stays its causes and those two lines,
/// never the schema itself.
#[test]
fn a_rejected_call_names_an_accepted_example_and_the_schema_command() {
    for tool in crate::id::ToolId::ALL {
        let tool = tool.as_str();
        let error = prepare_rows(tool, json!({"queries": [{"noSuchField": 1}]}))
            .map(|rows| {
                rows.into_iter()
                    .find_map(Result::err)
                    .unwrap_or_else(|| panic!("{tool} rejects an unknown field"))
            })
            .unwrap_or_else(|error| error);
        let envelope = format_input_error(tool, &error, &[]);
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
        assert_eq!(
            lines.last().copied(),
            Some(format!("Schema: octocode schema {tool}").as_str()),
            "{tool}: {lines:?}"
        );
        assert!(
            !envelope["error"].as_str().expect("cause").starts_with("Example"),
            "{tool}: the cause leads: {envelope}"
        );
        assert!(
            !envelope.to_string().contains("\"properties\""),
            "{tool}: the answer is not the schema: {envelope}"
        );
    }
    // A name that is no tool has no example to offer.
    let unknown = format_input_error("noSuchTool", &ContractValidationError { issues: vec![] }, &[]);
    assert!(unknown.get("details").is_none(), "{unknown}");
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
    let envelope = format_input_error("localSearch", &refused, &[]);
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
    let envelope = format_input_error("localSearch", &mixed, &[]);
    assert!(
        envelope["details"]
            .as_array()
            .is_some_and(|lines| lines.iter().any(|line| line
                .as_str()
                .is_some_and(|line| line.starts_with("Example: ")))),
        "{envelope}"
    );
}
