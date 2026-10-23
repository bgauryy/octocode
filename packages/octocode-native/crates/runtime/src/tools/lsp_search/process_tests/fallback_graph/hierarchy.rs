//! Fallback type-hierarchy shape: depth, trait impls, aliases, diamonds and cycles.

use super::*;

#[test]
fn fallback_type_hierarchy_obeys_depth_and_records_intermediate_identity() {
    let rows = fallback_graph_rows(
        &[(
            "chain.ts",
            "export class Base {}\nexport class Middle extends Base {}\nexport class Leaf extends Middle {}\n",
        )],
        &[
            ("chain.ts", 1, "Base", "chain.ts", 0),
            ("chain.ts", 2, "Middle", "chain.ts", 1),
        ],
        &[
            ("chain.ts", 0, &[("chain.ts", 1), ("chain.ts", 2)]),
            ("chain.ts", 1, &[("chain.ts", 2)]),
            ("chain.ts", 2, &[]),
        ],
        &[
            ("supertypes", "chain.ts", 2, 1),
            ("supertypes", "chain.ts", 2, 2),
            ("supertypes", "chain.ts", 2, 3),
            ("subtypes", "chain.ts", 0, 1),
            ("subtypes", "chain.ts", 0, 2),
            ("subtypes", "chain.ts", 0, 3),
        ],
    );
    for (index, row) in rows.iter().enumerate() {
        let matches = row["matches"].as_array().expect("matches");
        assert_eq!(
            matches.len(),
            if index % 3 == 0 { 1 } else { 2 },
            "{index}: {row}"
        );
        if index % 3 != 0 {
            assert_eq!(matches[1]["via"]["symbolName"], "Middle", "{row}");
        }
        assert!(
            row.get("isPartial").is_none(),
            "fully resolved graph: {row}"
        );
    }
}

/// rust-analyzer answers `implementation` on a trait with its impl blocks,
/// located at each self type: each trait impl is a direct subtype (its
/// clause names the trait), and an impl has no subtypes of its own.
#[test]
fn fallback_rust_subtypes_are_the_trait_impls() {
    let rows = fallback_graph_rows(
        &[(
            "lib.rs",
            "pub trait Scan {}\npub struct Foo {}\nimpl Scan for Foo {}\npub struct Bar {}\nimpl<T: Scan> Scan for Bar {}\nimpl Foo {}\n",
        )],
        &[
            ("lib.rs", 2, "Scan", "lib.rs", 0),
            ("lib.rs", 4, "Scan", "lib.rs", 0),
        ],
        &[
            ("lib.rs", 0, &[("lib.rs", 2), ("lib.rs", 4)]),
            // Asked at an impl's self type, a server lists that type's impls.
            ("lib.rs", 2, &[("lib.rs", 2), ("lib.rs", 5)]),
        ],
        &[("subtypes", "lib.rs", 0, 1), ("subtypes", "lib.rs", 0, 2)],
    );
    for row in rows {
        let found = row["matches"]
            .as_array()
            .expect("matches")
            .iter()
            .map(|found| (found["symbolName"].as_str(), found["kind"].as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            found,
            [(Some("Foo"), Some("impl")), (Some("Bar"), Some("impl"))],
            "{row}"
        );
        assert!(row.get("isPartial").is_none(), "{row}");
    }
}

#[test]
fn fallback_type_hierarchy_resolves_aliases_and_rejects_shadow_names() {
    let rows = fallback_graph_rows(
        &[
            ("base.ts", "export class Base {}\n"),
            (
                "alias.ts",
                "import { Base as Renamed } from './base';\nexport class Child extends Renamed {}\n",
            ),
            (
                "shadow.ts",
                "export class Base {}\nexport class Unrelated extends Base {}\n",
            ),
        ],
        &[
            ("alias.ts", 1, "Renamed", "base.ts", 0),
            ("shadow.ts", 1, "Base", "shadow.ts", 0),
        ],
        &[("base.ts", 0, &[("alias.ts", 1), ("shadow.ts", 1)])],
        &[
            ("supertypes", "alias.ts", 1, 1),
            ("subtypes", "base.ts", 0, 1),
        ],
    );
    assert_eq!(rows[0]["matches"].as_array().expect("matches").len(), 1);
    assert!(
        rows[0]["matches"][0]["path"]
            .as_str()
            .expect("path")
            .ends_with("base.ts"),
        "{}",
        rows[0]
    );
    assert_eq!(rows[1]["matches"].as_array().expect("matches").len(), 1);
    assert_eq!(rows[1]["matches"][0]["symbolName"], "Child", "{}", rows[1]);
    assert!(
        rows.iter().all(|row| row.get("isPartial").is_none()),
        "{rows:?}"
    );
}

#[test]
fn fallback_type_hierarchy_resolves_implementation_aliases_and_deduplicates_declarations() {
    let rows = fallback_graph_rows(
        &[
            (
                "chain.ts",
                "export interface Base {}\nexport class Middle implements Base {}\nexport class Leaf extends Middle {}\n",
            ),
            (
                "alias.ts",
                "import { Middle as Parent } from './chain';\nexport class AliasLeaf extends Parent {}\n",
            ),
            (
                "shadow.ts",
                "export interface Base {}\nexport class Middle implements Base {}\n",
            ),
        ],
        &[
            ("chain.ts", 1, "Base", "chain.ts", 0),
            ("chain.ts", 2, "Middle", "chain.ts", 1),
            ("alias.ts", 0, "Middle", "chain.ts", 1),
            ("alias.ts", 1, "Parent", "chain.ts", 1),
            ("shadow.ts", 1, "Base", "shadow.ts", 0),
        ],
        &[
            (
                "chain.ts",
                0,
                &[
                    ("alias.ts", 0),
                    ("chain.ts", 1),
                    ("chain.ts", 1),
                    ("chain.ts", 2),
                    ("alias.ts", 1),
                    ("shadow.ts", 1),
                ],
            ),
            (
                "chain.ts",
                1,
                &[
                    ("alias.ts", 0),
                    ("chain.ts", 2),
                    ("alias.ts", 1),
                    ("chain.ts", 2),
                ],
            ),
        ],
        &[
            ("subtypes", "chain.ts", 0, 1),
            ("subtypes", "chain.ts", 0, 2),
        ],
    );
    for (index, row) in rows.iter().enumerate() {
        assert!(
            row.get("isPartial").is_none(),
            "all alias candidates resolve: {row}"
        );
        let matches = row["matches"].as_array().expect("matches");
        assert_eq!(matches.len(), if index == 0 { 1 } else { 3 }, "{row}");
        assert_eq!(matches[0]["symbolName"], "Middle", "{row}");
        assert!(
            matches[0]["path"]
                .as_str()
                .expect("path")
                .ends_with("chain.ts"),
            "{row}"
        );
        if index == 1 {
            // A level lists its nodes by path, then position.
            assert_eq!(matches[1]["symbolName"], "AliasLeaf", "{row}");
            assert_eq!(matches[2]["symbolName"], "Leaf", "{row}");
            for child in &matches[1..] {
                assert_eq!(child["level"], 2, "{row}");
                assert_eq!(child["via"]["symbolName"], "Middle", "{row}");
            }
        }
    }
}

#[test]
fn fallback_type_hierarchy_keeps_known_edges_when_an_implementation_alias_is_ambiguous() {
    let rows = fallback_graph_rows(
        &[
            ("base.ts", "export interface Base {}\n"),
            ("left.ts", "export class Left implements Base {}\n"),
            ("right.ts", "export class Right implements Base {}\n"),
            ("alias.ts", "import { Either } from './left';\n"),
            ("same.ts", "import { Left as One } from './left';\n"),
        ],
        &[
            ("left.ts", 0, "Base", "base.ts", 0),
            ("right.ts", 0, "Base", "base.ts", 0),
            ("alias.ts", 0, "Either", "left.ts", 0),
            ("alias.ts", 0, "Either", "right.ts", 0),
            ("same.ts", 0, "Left", "left.ts", 0),
            ("same.ts", 0, "Left", "left.ts", 0),
        ],
        &[(
            "base.ts",
            0,
            &[("alias.ts", 0), ("same.ts", 0), ("left.ts", 0)],
        )],
        &[("subtypes", "base.ts", 0, 1)],
    );
    let row = &rows[0];
    let matches = row["matches"].as_array().expect("matches");
    assert_eq!(
        matches.len(),
        1,
        "the known declaration stays, ambiguous targets do not: {row}"
    );
    assert_eq!(matches[0]["symbolName"], "Left", "{row}");
    assert_eq!(row["isPartial"], true, "{row}");
    assert_eq!(
        row["partialReasons"],
        serde_json::json!(["typeHierarchyExpansionFailed"]),
        "{row}"
    );
    assert!(
        row["warnings"]
            .to_string()
            .contains("2 distinct type declarations"),
        "{row}"
    );
    // An ambiguous alias answers the same again: no retry.
    assert!(row.pointer("/next/retry").is_none(), "{row}");
}

#[test]
fn fallback_type_hierarchy_preserves_diamond_edges_and_terminates_cycles() {
    let diamond = fallback_graph_rows(
        &[(
            "diamond.ts",
            "export interface Root {}\nexport interface Left extends Root {}\nexport interface Right extends Root {}\nexport interface Join extends Left, Right {}\nexport interface Tail extends Join {}\n",
        )],
        &[
            ("diamond.ts", 1, "Root", "diamond.ts", 0),
            ("diamond.ts", 2, "Root", "diamond.ts", 0),
            ("diamond.ts", 3, "Left", "diamond.ts", 1),
            ("diamond.ts", 3, "Right", "diamond.ts", 2),
            ("diamond.ts", 4, "Join", "diamond.ts", 3),
        ],
        &[
            (
                "diamond.ts",
                0,
                &[
                    ("diamond.ts", 1),
                    ("diamond.ts", 2),
                    ("diamond.ts", 3),
                    ("diamond.ts", 4),
                ],
            ),
            ("diamond.ts", 1, &[("diamond.ts", 3), ("diamond.ts", 4)]),
            ("diamond.ts", 2, &[("diamond.ts", 3), ("diamond.ts", 4)]),
            ("diamond.ts", 3, &[("diamond.ts", 4)]),
        ],
        &[("subtypes", "diamond.ts", 0, 3)],
    );
    let matches = diamond[0]["matches"].as_array().expect("diamond matches");
    assert_eq!(matches.len(), 5, "{diamond:?}");
    assert_eq!(
        matches
            .iter()
            .filter(|entry| entry["symbolName"] == "Tail")
            .count(),
        1,
        "{diamond:?}"
    );
    let via = matches
        .iter()
        .filter(|entry| entry["symbolName"] == "Join")
        .map(|entry| entry["via"]["symbolName"].as_str().expect("via"))
        .collect::<Vec<_>>();
    assert_eq!(via, ["Left", "Right"], "{diamond:?}");
    let cycle = fallback_graph_rows(
        &[(
            "cycle.ts",
            "export class A extends B {}\nexport class B extends A {}\n",
        )],
        &[
            ("cycle.ts", 0, "B", "cycle.ts", 1),
            ("cycle.ts", 1, "A", "cycle.ts", 0),
        ],
        &[
            ("cycle.ts", 0, &[("cycle.ts", 0), ("cycle.ts", 1)]),
            ("cycle.ts", 1, &[("cycle.ts", 0), ("cycle.ts", 1)]),
        ],
        &[
            ("supertypes", "cycle.ts", 0, 20),
            ("subtypes", "cycle.ts", 0, 20),
        ],
    );
    for row in cycle {
        let matches = row["matches"].as_array().expect("cycle matches");
        assert_eq!(matches.len(), 2, "{row}");
        assert_eq!(matches[0]["symbolName"], "B", "{row}");
        assert_eq!(matches[1]["symbolName"], "A", "{row}");
        assert_eq!(matches[1]["via"]["symbolName"], "B", "{row}");
    }
}
