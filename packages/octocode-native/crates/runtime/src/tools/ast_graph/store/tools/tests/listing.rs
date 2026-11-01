//! `symbols` over file groups: one target, a list, or none (every symbol),
//! so a caller can walk a snapshot's symbols as concurrent groups whose union
//! is exactly the whole listing.
use super::*;

fn fixture() -> Fixture {
    let dir = Fixture::new();
    dir.write(
        "app/a.ts",
        "export class Box {\n  open() {}\n  close() {}\n}\nexport function a() {}\n",
    );
    dir.write("app/b.ts", "export function b() { function inner() {} }\n");
    dir.write("app/c.ts", "export const c = () => 1;\n");
    call(&dir.0, ToolId::GraphBuild, json!({"path":"app"}));
    dir
}

/// Every row id of `query`, following `next.page` to the end, and the first page's total.
fn walk(dir: &Path, mut query: Value) -> (Vec<String>, u64) {
    let mut ids = Vec::new();
    let mut total = None;
    loop {
        let page = call(dir, ToolId::GraphQuery, query);
        total.get_or_insert_with(|| {
            page["pagination"]["totalItems"]
                .as_u64()
                .unwrap_or_else(|| panic!("total: {page}"))
        });
        ids.extend(
            page["rows"]
                .as_array()
                .expect("results")
                .iter()
                .map(|row| row["id"].as_str().expect("id").to_owned()),
        );
        match page.pointer("/next/nextPage/input/queries/0") {
            Some(next) => query = next.clone(),
            None => return (ids, total.expect("a page")),
        }
    }
}

#[test]
fn symbol_groups_add_up_to_every_symbol_exactly_once() {
    let dir = fixture();
    let (all, total) = walk(
        &dir.0,
        json!({"operation":"symbols","path":"app","limit":2}),
    );
    assert_eq!(all.len() as u64, total);
    assert_eq!(
        all.iter().collect::<BTreeSet<_>>().len(),
        all.len(),
        "{all:?}"
    );
    let (found, found_total) = walk(
        &dir.0,
        json!({"operation":"find","path":"app","kind":"symbol","limit":1000}),
    );
    assert_eq!(total, found_total, "every symbol, as find lists them");
    assert_eq!(
        all.iter().collect::<BTreeSet<_>>(),
        found.iter().collect::<BTreeSet<_>>()
    );
    let (first, first_total) = walk(
        &dir.0,
        json!({"operation":"symbols","path":"app","target":["a.ts","b.ts"],"limit":2}),
    );
    let (rest, rest_total) = walk(
        &dir.0,
        json!({"operation":"symbols","path":"app","target":["c.ts"],"limit":2}),
    );
    assert_eq!(first_total + rest_total, total);
    let union = first.iter().chain(&rest).collect::<BTreeSet<_>>();
    assert_eq!(union.len(), all.len());
    assert_eq!(union, all.iter().collect::<BTreeSet<_>>());
    let (one, _) = walk(
        &dir.0,
        json!({"operation":"symbols","path":"app","target":"a.ts"}),
    );
    assert_eq!(
        one,
        ["a.ts#Box", "a.ts#Box.open", "a.ts#Box.close", "a.ts#a"]
    );
}

#[test]
fn a_symbol_under_two_targets_is_listed_once() {
    let dir = fixture();
    let (rows, total) = walk(
        &dir.0,
        json!({"operation":"symbols","path":"app","target":["a.ts","a.ts#Box"]}),
    );
    assert_eq!(
        rows,
        ["a.ts#Box", "a.ts#Box.open", "a.ts#Box.close", "a.ts#a"]
    );
    assert_eq!(total, 4);
}
