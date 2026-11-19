//! F3 omitted values: a field holding the value its rule derives is
//! omitted, and reads back as that value; any other value stays.
use super::*;

fn symbol(name: &str, identity: Value) -> Value {
    json!({"symbolName": name, "kind": "function", "line": 4, "testIdentity": identity})
}

/// Responses C1: a Rust test's `testIdentity` repeats its symbol's name, a
/// `parameterized: false` and a `kind: declaration`; all three are omitted,
/// and a test whose values differ keeps them.
#[test]
fn constants_and_values_equal_to_a_sibling_are_omitted() {
    let plan = super::plan(
        vec![
            omitted(
                "/files/*/symbols/*/testIdentity/name",
                When::Equals(Pattern::parse("/files/*/symbols/*/symbolName")),
            ),
            omitted(
                "/files/*/symbols/*/testIdentity/parameterized",
                When::Const(json!(false)),
            ),
            omitted(
                "/files/*/symbols/*/testIdentity/kind",
                When::Const(json!("declaration")),
            ),
        ],
        Vec::new(),
        Batch::default(),
    );
    let plain = json!({"framework": "rust", "name": "reads_twice", "parameterized": false, "kind": "declaration"});
    let own = json!({"framework": "rust", "name": "other", "parameterized": true, "kind": "case"});
    let original = json!({"results": [{"index": 0, "data": {"files": [{"path": "a.rs", "symbols": [
        symbol("reads_twice", plain),
        symbol("cases", own.clone()),
    ]}]}}]});
    let stated = super::hoisted(&plan, &original, &super::optional);
    let symbols = &stated["results"][0]["data"]["files"][0]["symbols"];
    assert_eq!(
        symbols[0]["testIdentity"],
        json!({"framework": "rust"}),
        "{stated}"
    );
    assert_eq!(symbols[1]["testIdentity"], own, "{stated}");
    super::assert_recovers(&plan, &original, &stated);
}

/// Hoist K-8: an advisory `url` that is the OSV link its `id` spells is
/// omitted; a url elsewhere stays.
#[test]
fn a_value_its_template_spells_is_omitted() {
    let plan = super::plan(
        vec![omitted(
            "/advisories/*/url",
            When::Template(vec![
                Piece::Text("https://osv.dev/vulnerability/".into()),
                Piece::Field(Pattern::parse("/advisories/*/id")),
            ]),
        )],
        Vec::new(),
        Batch::default(),
    );
    let original = json!({"results": [{"index": 0, "data": {"advisories": [
        {"id": "GHSA-1", "url": "https://osv.dev/vulnerability/GHSA-1"},
        {"id": "GHSA-2", "url": "https://github.com/advisories/GHSA-2"},
    ]}}]});
    let stated = super::hoisted(&plan, &original, &super::optional);
    let advisories = &stated["results"][0]["data"]["advisories"];
    assert!(advisories[0].get("url").is_none(), "{stated}");
    assert_eq!(advisories[1]["url"], "https://github.com/advisories/GHSA-2");
    super::assert_recovers(&plan, &original, &stated);
}

/// A field whose source is absent stays: nothing could derive it. And no
/// rule reaches into a call a reader runs.
#[test]
fn a_value_without_its_source_or_inside_a_call_stays() {
    let plan = super::plan(
        vec![
            omitted(
                "/reviews/*/commitSha",
                When::Equals(Pattern::parse("/headCommitSha")),
            ),
            omitted("/next/nextPage/input/side", When::Const(json!("RIGHT"))),
        ],
        Vec::new(),
        Batch::default(),
    );
    let original = json!({"results": [{"index": 0, "data": {
        "reviews": [{"id": 1, "commitSha": "c1"}],
        "next": {"nextPage": {"tool": "ghGetHistoryItem", "input": {"side": "RIGHT"}}},
    }}]});
    assert_eq!(super::hoisted(&plan, &original, &super::optional), original);
    let mut headed = original.clone();
    headed["results"][0]["data"]["headCommitSha"] = json!("c1");
    let stated = super::hoisted(&plan, &headed, &super::optional);
    assert!(
        stated["results"][0]["data"]["reviews"][0]
            .get("commitSha")
            .is_none()
    );
    assert_eq!(
        stated["results"][0]["data"]["next"],
        headed["results"][0]["data"]["next"]
    );
    super::assert_recovers(&plan, &headed, &stated);
}

/// A template's holes are row pointers in braces; text between them stays.
#[test]
fn a_template_reads_its_holes_as_row_pointers() {
    let pieces =
        super::super::plan::pieces("https://osv.dev/vulnerability/{/data/advisories/*/id}");
    assert!(
        matches!(&pieces[..], [Piece::Text(text), Piece::Field(field)]
        if text == "https://osv.dev/vulnerability/" && field == &Pattern::parse("/advisories/*/id"))
    );
}

/// A tool that prices one item prices it as the stage publishes it: the
/// rules below the item apply; a rule whose source lies outside it keeps
/// the value, so the item is never priced shorter than it is published.
#[test]
fn an_item_priced_alone_drops_what_its_own_fields_derive() {
    let entries = [
        omitted(
            "/files/*/symbols/*/testIdentity/name",
            When::Equals(Pattern::parse("/files/*/symbols/*/symbolName")),
        ),
        omitted(
            "/files/*/symbols/*/kind",
            When::Equals(Pattern::parse("/files/*/kind")),
        ),
    ];
    let prefix = Pattern::parse("/files/*/symbols/*");
    let within = entries
        .iter()
        .filter_map(|entry| entry.within(&prefix))
        .collect::<Vec<_>>();
    assert_eq!(
        within.len(),
        1,
        "the rule reaching outside the item is left out"
    );
    let mut item = symbol("reads", json!({"framework": "rust", "name": "reads"}));
    super::super::omit::apply(&within, &mut item);
    assert_eq!(item["testIdentity"], json!({"framework": "rust"}));
    assert_eq!(item["kind"], "function");
}

/// Hoist C-7: a count that only restates its list's length is omitted, and
/// reads back as that length; a count that differs stays.
#[test]
fn a_count_equal_to_its_lists_length_is_omitted() {
    let plan = super::plan(
        vec![omitted(
            "/files/*/matchCount",
            When::CountOf(Pattern::parse("/files/*/matchedLines")),
        )],
        Vec::new(),
        Batch::default(),
    );
    let original = json!({"results": [{"index": 0, "data": {"files": [
        {"path": "a.rs", "matchCount": 2, "matchedLines": [3, 9]},
        {"path": "b.rs", "matchCount": 3, "matchedLines": [4, 4]},
    ]}}]});
    let stated = super::hoisted(&plan, &original, &super::optional);
    assert!(
        stated["results"][0]["data"]["files"][0]
            .get("matchCount")
            .is_none(),
        "{stated}"
    );
    assert_eq!(stated["results"][0]["data"]["files"][1]["matchCount"], 3);
    super::assert_recovers(&plan, &original, &stated);
}

/// An entry planned on top-level declarations applies to their `members`
/// at any depth (core `collectionRefs`), re-rooted on each member.
#[test]
fn an_omitted_value_applies_through_nested_members_at_any_depth() {
    let mut plan = super::plan(
        vec![omitted(
            "/symbols/*/testIdentity/name",
            When::Equals(Pattern::parse("/symbols/*/symbolName")),
        )],
        Vec::new(),
        Batch::default(),
    );
    plan.refs.push(super::super::plan::Recurse {
        target: Pattern::parse("/symbols/*"),
        tail: Pattern::parse("/members/*"),
    });
    let test = |name: &str| json!({"framework": "rust", "name": name});
    let original = json!({"results": [{"index": 0, "data": {"symbols": [{
        "symbolName": "tests", "kind": "module", "line": 1, "testIdentity": test("tests"),
        "members": [{
            "symbolName": "inner", "kind": "module", "line": 2, "testIdentity": test("inner"),
            "members": [
                {"symbolName": "reads", "kind": "function", "line": 3, "testIdentity": test("reads")},
                {"symbolName": "named", "kind": "function", "line": 9, "testIdentity": test("other")},
            ],
        }],
    }]}}]});
    let stated = super::hoisted(&plan, &original, &super::optional);
    let top = &stated["results"][0]["data"]["symbols"][0];
    assert_eq!(
        top["testIdentity"],
        json!({"framework": "rust"}),
        "{stated}"
    );
    assert_eq!(
        top["members"][0]["testIdentity"],
        json!({"framework": "rust"}),
        "{stated}"
    );
    let deepest = &top["members"][0]["members"];
    assert_eq!(
        deepest[0]["testIdentity"],
        json!({"framework": "rust"}),
        "{stated}"
    );
    assert_eq!(
        deepest[1]["testIdentity"],
        test("other"),
        "a differing name stays"
    );
}
