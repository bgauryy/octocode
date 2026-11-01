//! A row split several levels deep (file > symbol > members) joins back
//! to the row exactly once when its parts are concatenated as the
//! contract says: `rowPart.continues` names the innermost continued
//! element, in the joined row's coordinates, and every element on its
//! pointer's path continues too.
use super::*;
use crate::id::ToolId;
use tokio_util::sync::CancellationToken;

fn symbols_row() -> Value {
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
fn continued_elements(marker: &Value) -> Vec<(String, usize)> {
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

/// Concatenate `part` onto `joined` at pointer `at`: a continued array's
/// first item merges into the element the marker names, which must be the
/// last one joined so far; any other item is appended; repeated context
/// scalars must agree.
fn merge(joined: &mut Value, part: Value, at: &str, continued: &[(String, usize)]) {
    match (joined, part) {
        (Value::Object(joined), Value::Object(part)) => {
            for (key, value) in part {
                let pointer = format!("{at}/{key}");
                match joined.get_mut(&key) {
                    Some(slot) => merge(slot, value, &pointer, continued),
                    None => {
                        joined.insert(key, value);
                    }
                }
            }
        }
        (Value::Array(joined), Value::Array(part)) => {
            let mut items = part.into_iter();
            if let Some((_, index)) = continued.iter().find(|(array, _)| array == at) {
                assert_eq!(*index + 1, joined.len(), "{at}: the marker names the last element");
                let first = items.next().expect("a continued array resumes its element");
                merge(&mut joined[*index], first, &format!("{at}/{index}"), continued);
            }
            joined.extend(items);
        }
        (joined, part) => assert_eq!(*joined, part, "{at}: repeated context changed"),
    }
}

#[test]
fn parts_split_several_levels_deep_join_back_exactly_once() {
    let data = symbols_row();
    let mut nested = false;
    for budget in (60..=400).step_by(5) {
        let plan = PreparedRows::new(
            vec![json!({"index":0,"data":data.clone()})],
            &[None],
            ToolId::AstSearch,
        )
        .plan(budget, &CancellationToken::new())
        .expect("valid plan");
        let mut joined: Option<Value> = None;
        for page in 0..plan.total_pages {
            for row in plan
                .build_page(page, &[None], &[], &CancellationToken::new())
                .expect("valid page")
            {
                let marker = &row["rowPart"]["continues"];
                nested |= marker["array"].as_str().is_some_and(|array| array.contains("/symbols"));
                let continued = continued_elements(marker);
                match joined.as_mut() {
                    None => joined = Some(row["data"].clone()),
                    Some(joined) => merge(joined, row["data"].clone(), "/data", &continued),
                }
            }
        }
        assert_eq!(joined.as_ref(), Some(&data), "budget {budget}");
    }
    assert!(nested, "some budget splits a symbol inside a split file");
}
