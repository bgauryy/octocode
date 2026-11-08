//! A row split several levels deep (file > symbol > members) joins back
//! to the row exactly once when its parts are concatenated as the
//! contract says: `rowPart.continues` names the innermost continued
//! element, in the joined row's coordinates, and every element on its
//! pointer's path continues too.
use super::*;
use crate::id::ToolId;
use octocode_io::cancellation::Cancellation;

pub(super) fn symbols_row() -> Value {
    json!({"filesScanned":3,"files":[
        {"path":"src/app.ts","symbols":[{"symbolName":"main","kind":"function","line":5,"endLine":9,"exported":true,"docStartLine":4}]},
        {"path":"src/greeter.ts","symbols":[
            {"symbolName":"Greeting","kind":"interface","line":2,"endLine":4,"exported":true,"docStartLine":1,
             "members":[{"symbolName":"text","kind":"property","line":3}]},
            {"symbolName":"greet","kind":"function","line":6,"endLine":9,"exported":true},
            {"symbolName":"Greeter","kind":"class","line":11,"endLine":18,"exported":true,
             "members":[{"symbolName":"constructor","kind":"constructor","line":12},
                        {"symbolName":"say","kind":"method","line":14,"endLine":17}]}]},
        {"path":"src/util.ts","symbols":[
            {"symbolName":"shout","kind":"function","line":4,"endLine":6,"exported":true,"docStartLine":3},
            {"symbolName":"greetLoudly","kind":"function","line":8,"endLine":10,"exported":true}]}]})
}

/// The continued elements a marker names: each array element on its
/// pointer's path, then the element it names, as (array pointer, index).
pub(super) fn continued_elements(marker: &Value) -> Vec<(String, usize)> {
    let Some(array) = marker["array"].as_str() else {
        return Vec::new();
    };
    let mut elements = Vec::new();
    let mut prefix = String::new();
    for segment in array.trim_start_matches('/').split('/') {
        if let Ok(index) = segment.parse::<usize>() {
            elements.push((prefix.clone(), index));
        }
        prefix = format!("{prefix}/{segment}");
    }
    let index = marker["index"].as_u64().expect("marker index");
    elements.push((array.to_owned(), usize::try_from(index).expect("index")));
    elements
}

/// What a joiner knows of one later part: the elements its marker continues
/// (`rowPart.continues`), the string it appends to (`rowPart.text`), and the
/// row's declared collections (the contract's `collectionPaths`, as concrete
/// pointers of the unsplit row), whose items a part appends.
pub(super) struct Join<'a> {
    pub(super) continued: &'a [(String, usize)],
    pub(super) text: Option<&'a str>,
    pub(super) collections: &'a [String],
}

/// The concrete pointers of the declared collections in `data` (a row's
/// `data`), from the contract's collection routes alone.
pub(super) fn declared_collections(tool: ToolId, data: &Value) -> Vec<String> {
    let routes = &crate::contracts::output_fields::page_context_fields(tool).collection_routes;
    super::super::collections::collection_paths(&json!({ "data": data }), routes)
}

/// Concatenate `part` onto `joined` at pointer `at`: a continued array's
/// first item merges into the element the marker names, which must be the
/// last one joined so far; a declared collection's other items are
/// appended; the string `rowPart.text` names is appended to; an empty
/// string or list adds nothing (required evidence a later part carries only
/// to validate); any other repeated value (a scalar, a bounded string, a
/// list or record the schema forbids empty) must agree and is kept once.
pub(super) fn merge(joined: &mut Value, part: Value, at: &str, join: &Join<'_>) {
    match (joined, part) {
        (Value::Object(joined), Value::Object(part)) => {
            for (key, value) in part {
                let pointer = format!("{at}/{}", crate::json_pointer::escape(&key));
                match joined.get_mut(&key) {
                    Some(slot) => merge(slot, value, &pointer, join),
                    None => {
                        joined.insert(key, value);
                    }
                }
            }
        }
        (Value::Array(joined), Value::Array(part)) => {
            let mut items = part.into_iter();
            if let Some((_, index)) = join.continued.iter().find(|(array, _)| array == at) {
                assert_eq!(
                    *index + 1,
                    joined.len(),
                    "{at}: the marker names the last element"
                );
                let first = items.next().expect("a continued array resumes its element");
                merge(&mut joined[*index], first, &format!("{at}/{index}"), join);
            } else if items.len() > 0 && !join.collections.iter().any(|path| path == at) {
                let repeated = items.collect::<Vec<_>>();
                assert_eq!(*joined, repeated, "{at}: a repeated list changed");
                return;
            }
            joined.extend(items);
        }
        (Value::String(joined), Value::String(part)) if join.text == Some(at) => {
            joined.push_str(&part);
        }
        (Value::String(_), Value::String(part)) if part.is_empty() => {}
        (joined, part) => assert_eq!(*joined, part, "{at}: repeated context changed"),
    }
}

#[test]
fn parts_split_several_levels_deep_join_back_exactly_once() {
    let data = symbols_row();
    let collections = declared_collections(ToolId::AstSearch, &data);
    let mut nested = false;
    for budget in (60..=400).step_by(5) {
        let plan = PreparedRows::new(
            vec![json!({"index":0,"data":data.clone()})],
            ToolId::AstSearch,
        )
        .plan(budget, &Cancellation::new())
        .expect("valid plan");
        let mut joined: Option<Value> = None;
        for page in 0..plan.total_pages {
            for row in plan
                .build_page(page, &[None], &Cancellation::new())
                .expect("valid page")
            {
                let marker = &row["rowPart"]["continues"];
                nested |= marker["array"]
                    .as_str()
                    .is_some_and(|array| array.contains("/symbols"));
                let continued = continued_elements(marker);
                match joined.as_mut() {
                    None => joined = Some(row["data"].clone()),
                    Some(joined) => merge(
                        joined,
                        row["data"].clone(),
                        "/data",
                        &Join {
                            continued: &continued,
                            text: row["rowPart"]["text"].as_str(),
                            collections: &collections,
                        },
                    ),
                }
            }
        }
        assert_eq!(joined.as_ref(), Some(&data), "budget {budget}");
    }
    assert!(nested, "some budget splits a symbol inside a split file");
}
