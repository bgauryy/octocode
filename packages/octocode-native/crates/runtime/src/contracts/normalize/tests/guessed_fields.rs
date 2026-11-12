use super::*;

/// A field agents send for a declared one, with one reading, runs as that
/// field, and the call says which field it read and which to send.
#[test]
fn a_guessed_field_with_one_reading_runs_and_is_named() {
    let row = |sent: Value| json!({"queries":[sent]});
    for (tool, sent, field, expected) in [
        (
            "localSearch",
            json!({"path":".","pattern":"x"}),
            "matchString",
            json!(["x"]),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","mode":"literal"}),
            "regexEngine",
            json!("literal"),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","mode":"pcre2"}),
            "regexEngine",
            json!("pcre2"),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","literal":true}),
            "regexEngine",
            json!("literal"),
        ),
        // The former search mode that listed matching files only.
        (
            "localSearch",
            json!({"path":".","matchString":"x","mode":"discovery"}),
            "view",
            json!("files"),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","filesOnly":true}),
            "view",
            json!("files"),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","countOnly":true}),
            "view",
            json!("countMatches"),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","perFileCount":true}),
            "view",
            json!("countMatches"),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","excludeDir":"tests"}),
            "exclude",
            json!(["tests"]),
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","exclude":["a"],"excludeDir":["b"]}),
            "exclude",
            json!(["a", "b"]),
        ),
        (
            "localFetch",
            json!({"path":"a.go","startLine":1530,"endLine":1560}),
            "ranges",
            json!([{"startLine":1530,"endLine":1560}]),
        ),
        (
            "localFetch",
            json!({"path":"a.go","ranges":[[1530,1626]]}),
            "ranges",
            json!([{"startLine":1530,"endLine":1626}]),
        ),
        (
            "ghGetFileContent",
            json!({"owner":"o","repo":"r","path":"a.go","startLine":3,"endLine":9}),
            "ranges",
            json!([{"startLine":3,"endLine":9}]),
        ),
        (
            "ghSearchHistory",
            json!({"owner":"o","repo":"r","type":"commit"}),
            "operation",
            json!("commit"),
        ),
        (
            "ghSearchCode",
            json!({"owner":"o","keywords":["x"],"match":"content"}),
            "matchIn",
            json!(["content"]),
        ),
    ] {
        let (input, notes) = normalized_input(tool, row(sent.clone()));
        assert_eq!(
            input["queries"][0][field], expected,
            "{tool} {sent}: {input}"
        );
        let rows = prepare_rows(tool, input.clone()).unwrap_or_else(|error| {
            panic!("{tool} {sent} should run: {:?}", error.issues);
        });
        assert!(rows.iter().all(Result::is_ok), "{tool} {sent}: {rows:?}");
        assert_eq!(notes.len(), 1, "{notes:?}");
        // The one note names the field the call ran as and the one to send.
        assert!(
            notes[0].starts_with("queries[0]: read ")
                && notes[0].contains(&format!("send `{field}")),
            "{notes:?}"
        );
        // A field read with a value other than any sent (`filesOnly: true`
        // is `view: "files"`) names that value, so the repair is whole; a
        // sent text that validation lists (`"x"` as `["x"]`) is the same value.
        let sent_values = sent.as_object().into_iter().flat_map(|row| row.values());
        if field != "ranges"
            && !sent_values
                .into_iter()
                .any(|value| *value == expected || json!([value]) == expected)
        {
            assert!(
                notes[0].contains(&format!("send `{field}: {expected}` next time")),
                "{notes:?}"
            );
        }
    }
}

/// A guess with more than one reading stays rejected, and the error names the
/// field to use.
#[test]
fn a_guessed_field_with_several_readings_names_the_field_to_use() {
    for (tool, sent, sent_field, meant) in [
        (
            "localSearch",
            json!({"path":".","matchString":"x","mode":"regex"}),
            "mode",
            "regexEngine",
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","literal":false}),
            "literal",
            "regexEngine",
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","view":"files","countOnly":true}),
            "countOnly",
            "view",
        ),
        (
            "localSearch",
            json!({"path":".","matchString":"x","pattern":"y"}),
            "pattern",
            "matchString",
        ),
        (
            "localFetch",
            json!({"path":"a.go","startLine":4}),
            "startLine",
            "ranges",
        ),
        (
            "localFetch",
            json!({"path":"a.go","ranges":[2,4]}),
            "ranges",
            "ranges",
        ),
        // Former names of renamed fields.
        (
            "structureSearch",
            json!({"operation":"files","path":".","sizeBytes":{"min":1}}),
            "sizeBytes",
            "bytes",
        ),
        (
            "lspSearch",
            json!({"operation":"workspaceSymbol","path":"src","symbolQuery":"run"}),
            "symbolQuery",
            "symbolFuzzy",
        ),
        (
            "graphQuery",
            json!({"operation":"unresolved","name":"run"}),
            "name",
            "symbolName",
        ),
    ] {
        let (input, notes) = normalized_input(tool, json!({"queries":[sent.clone()]}));
        assert!(notes.is_empty(), "{tool} {sent}: {notes:?}");
        let error = match prepare_rows(tool, input) {
            Err(error) => error,
            Ok(mut rows) => rows.remove(0).expect_err("rejected"),
        };
        if sent_field == meant {
            continue;
        }
        let shown =
            crate::contracts::format_input_error(tool, &error, &[], &serde_json::Value::Null)
                .to_string();
        assert!(
            shown.contains(&format!(
                "Unknown field '{sent_field}' in queries[0]; use '{meant}'"
            )),
            "{tool} {sent}: {shown}"
        );
    }
}

/// `pattern` names a regex in most tools and `matchString` is literal text
/// unless `regexEngine` says otherwise: a value that also reads as a regex
/// has two readings, so it is refused with both fields named. A value with
/// no regex reading (no regex syntax, or syntax that does not compile) is
/// the literal it spells.
#[test]
fn a_pattern_that_reads_as_a_regex_is_refused_naming_both_fields() {
    for value in [
        json!("export|import"),
        json!("x[0-9]"),
        json!("foo.bar"),
        json!(["plain", "a|b"]),
    ] {
        let sent = json!({"path":".","pattern":value});
        let (input, notes) = normalized_input("localSearch", json!({"queries":[sent.clone()]}));
        assert!(notes.is_empty(), "{sent}: {notes:?}");
        assert_eq!(input["queries"][0], sent, "left as sent");
        let error = match prepare_rows("localSearch", input) {
            Err(error) => error,
            Ok(mut rows) => rows.remove(0).expect_err("rejected"),
        };
        let shown = crate::contracts::format_input_error(
            "localSearch",
            &error,
            &[],
            &serde_json::Value::Null,
        )["error"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        assert!(
            shown.starts_with("Unknown field 'pattern' in queries[0]; use 'matchString'")
                && shown.contains("literal")
                && shown.contains(r#"regexEngine:"rust""#),
            "{sent}: {shown}"
        );
    }
    for (value, read) in [
        (json!("plain words"), json!(["plain words"])),
        (json!("call("), json!(["call("])),
        (json!(["a", "b"]), json!(["a", "b"])),
    ] {
        let sent = json!({"path":".","pattern":value});
        let (input, notes) = normalized_input("localSearch", json!({"queries":[sent.clone()]}));
        assert_eq!(input["queries"][0]["matchString"], read, "{sent}");
        assert_eq!(notes.len(), 1, "{sent}: {notes:?}");
    }
    // The engine already says how the value is read: one reading.
    let sent = json!({"path":".","pattern":"export|import","regexEngine":"rust"});
    let (input, notes) = normalized_input("localSearch", json!({"queries":[sent]}));
    assert_eq!(input["queries"][0]["matchString"], json!(["export|import"]));
    assert_eq!(notes.len(), 1, "{notes:?}");
}
