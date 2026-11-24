//! What the example beside a refusal shows, and which coercions warn: the
//! example keeps a valid value where the row's own cannot stand, invents no
//! real-looking value, and a coercion is always said.
use super::repair_tests::{answered, shown_example};
use crate::contracts::normalized_input;
use serde_json::{Value, json};

fn example_line(lines: &[String]) -> Option<&String> {
    lines.iter().find(|line| line.starts_with("Example: "))
}

/// A text outside an enum that no sibling or near spelling explains is shown
/// as the allowed member nearest to it in spelling, never dropped with its
/// field: `merged` for an issue search is nearest to `closed`.
#[test]
fn a_refused_enum_text_is_shown_as_its_nearest_member() {
    let shown = shown_example(
        "ghSearchHistory",
        json!({"operation":"issue","owner":"pallets","repo":"flask","filters":{"state":"merged"}}),
    );
    assert_eq!(shown["filters"]["state"], "closed", "{shown}");
}

/// A required number the row never sent has no placeholder that cannot read
/// as a real one (`1` is a real issue number), so no example is shown and
/// the refusal names the field.
#[test]
fn a_missing_required_number_is_named_not_invented() {
    let sent =
        json!({"queries":[{"operation":"pullRequest","owner":"expressjs","repo":"express"}]});
    let (_, lines) = answered("ghGetHistoryItem", &sent);
    assert!(lines[0].contains("number"), "{lines:?}");
    assert!(example_line(&lines).is_none(), "{lines:?}");
}

/// A scalar sent for an object whose fields it fits equally is shown under
/// the first field that takes it, so the example holds the value sent.
#[test]
fn a_scalar_for_an_object_of_several_fields_gets_an_example() {
    let shown = shown_example(
        "astSearch",
        json!({"operation":"match","path":".","rule":"identifier"}),
    );
    let rule = shown["rule"].as_object().expect("an object rule");
    assert!(rule.values().any(|value| value == "identifier"), "{shown}");
}

/// A number written as an integral float is read as the integer, silently:
/// the reading changes no value.
#[test]
fn an_integral_float_for_an_integer_is_read_silently() {
    let sent = json!({"queries":[{"path":".","matchString":["a"],"limit":3.0}]});
    let (read, warnings) = normalized_input("localSearch", sent);
    assert_eq!(read["queries"][0]["limit"], json!(3), "{read}");
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// An out-of-range value for a field the contract says how to find is
/// refused with that guidance, as a missing one is.
#[test]
fn an_out_of_range_line_says_how_to_find_it() {
    for line in [json!(0), json!(-3)] {
        let sent = json!({"queries":[{"operation":"definition","path":"a.ts","symbolName":"x","line":line}]});
        let (_, lines) = answered("lspSearch", &sent);
        assert!(
            lines[0].contains("localSearch or astSearch"),
            "{line}: {lines:?}"
        );
        let _: &Value = &line;
    }
}

/// A rule the fix of an earlier one already satisfies is no second cause:
/// `owner`, which `repo` requires, also makes a search runnable, so the
/// refusal names `owner` alone and the example adds no keyword placeholder.
/// Requirements no single fix meets stay, each named.
#[test]
fn a_rule_the_first_fix_satisfies_is_not_a_second_cause() {
    let row = json!({"operation":"search","repo":"flask"});
    let (_, lines) = answered("ghSearchRepo", &json!({"queries":[row.clone()]}));
    assert!(lines[0].contains("repo requires owner"), "{lines:?}");
    assert!(
        !lines.iter().any(|line| line.contains("requires keywords")),
        "{lines:?}"
    );
    let shown = shown_example("ghSearchRepo", row);
    assert!(shown.get("keywords").is_none(), "{shown}");
    assert_eq!(shown["repo"], "flask", "{shown}");
    let shown = shown_example("ghSearchRepo", json!({"operation":"search"}));
    assert!(shown.get("keywords").is_some(), "{shown}");
}

/// A key another operation takes in the same field belongs to that
/// operation: the refusal names it, and the example runs the row under it
/// with the key kept, never the row emptied of what it asked
/// (`filters: {}`), which would silently drop the filter.
#[test]
fn a_key_of_another_operation_redirects_to_that_operation() {
    let row = json!({"operation":"list","owner":"pallets","filters":{"language":"Python"}});
    let (_, lines) = answered("ghSearchRepo", &json!({"queries":[row.clone()]}));
    assert!(
        lines[0].contains(r#"requires operation:"search""#),
        "{lines:?}"
    );
    let shown = shown_example("ghSearchRepo", row);
    assert_eq!(
        shown,
        json!({"operation":"search","owner":"pallets","filters":{"language":"Python"}})
    );
    let shown = shown_example(
        "ghSearchRepo",
        json!({"operation":"list","owner":"pallets","keywords":["web"]}),
    );
    assert_eq!(
        shown,
        json!({"operation":"search","owner":"pallets","keywords":["web"]})
    );
    // A field the other operation refuses (search sorts by no `pushedAt`)
    // is left out; the redirected filter stays.
    let shown = shown_example(
        "ghSearchRepo",
        json!({"operation":"list","owner":"pallets","filters":{"language":"Python"},
               "sort":{"by":"pushedAt","order":"desc"}}),
    );
    assert_eq!(
        shown,
        json!({"operation":"search","owner":"pallets","filters":{"language":"Python"}})
    );
}

/// A list entry outside its enum is never shown as a value the list already
/// holds (`commitSha` for a pull request is not `title` twice): the entry is
/// left out. An entry whose nearest member the list lacks is that member.
#[test]
fn a_refused_list_entry_never_repeats_a_held_value() {
    let row = |fields: Value| json!({"operation":"pullRequest","owner":"pallets","repo":"click","keywords":["fix"],"fields":fields});
    let shown = shown_example(
        "ghSearchHistory",
        row(json!([
            "number",
            "title",
            "author",
            "mergedAt",
            "commitSha"
        ])),
    );
    assert_eq!(
        shown["fields"],
        json!(["number", "title", "author", "mergedAt"]),
        "{shown}"
    );
    let shown = shown_example("ghSearchHistory", row(json!(["number", "titel"])));
    assert_eq!(shown["fields"], json!(["number", "title"]), "{shown}");
}
