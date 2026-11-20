//! Explainable weights: `sites` on edge rows and hubs, `rankBy:"pageRank"`,
//! and the witness and linkage counts they reconcile with.
use super::*;

/// `mixed` calls `zeta` three times on one line and `alpha` once; `top`
/// calls `mixed` once and a builtin no declaration in the scope names.
fn weighted() -> Fixture {
    let dir = git_app(&[
        (
            "a.ts",
            "export function alpha() { return 1; }\nexport function zeta() { return 2; }\n",
        ),
        (
            "b.ts",
            "import { alpha, zeta } from './a';\nexport function mixed() { return zeta() + zeta() + zeta() + alpha(); }\n",
        ),
        (
            "c.ts",
            "import { mixed } from './b';\nexport function top() { return mixed() + Math.max(1, 2); }\n",
        ),
    ]);
    assert_eq!(
        ingest_fixture(dir.0.as_path(), None).class,
        ExitClass::Success
    );
    dir
}

fn rows(out: &GraphOutput) -> Vec<Value> {
    out.value["rows"].as_array().cloned().unwrap_or_default()
}

fn row<'a>(rows: &'a [Value], id: &str) -> &'a Value {
    rows.iter()
        .find(|row| row["id"] == id)
        .unwrap_or_else(|| panic!("no row {id}: {rows:?}"))
}

fn sites(rows: &[Value]) -> u64 {
    rows.iter().map(|row| row["sites"].as_u64().unwrap()).sum()
}

/// Every row of an answer, walked page by page.
fn all_rows(root: &Path, op: &str, tweak: impl Fn(&mut QueryOptions)) -> (Vec<Value>, Value) {
    let mut collected = Vec::new();
    let mut first = Value::Null;
    loop {
        let out = ask(root, op, None, |o| {
            tweak(o);
            o.limit = Some(2);
            o.offset = Some(collected.len());
        });
        assert!(
            matches!(out.class, ExitClass::Success | ExitClass::Empty),
            "{}",
            out.value
        );
        if first.is_null() {
            first = out.value.clone();
        }
        let page = rows(&out);
        let total = out.value["pagination"]["total"]
            .as_u64()
            .map_or(page.len(), |total| total as usize);
        collected.extend(page);
        if collected.len() >= total {
            return (collected, first);
        }
    }
}

/// An edge row counts the distinct source occurrences behind its edge.
#[test]
fn edge_rows_count_the_sites_behind_their_edge() {
    let dir = weighted();
    let root = dir.0.as_path();
    for (callee, expected) in [("app/a.ts#zeta", 3), ("app/a.ts#alpha", 1)] {
        let out = ask(root, "callers", Some(callee), |_| {});
        let listed = rows(&out);
        assert_eq!(
            row(&listed, "app/b.ts#mixed")["sites"],
            expected,
            "{}",
            out.value
        );
    }
    let deps = ask(root, "deps", Some("c.ts"), |_| {});
    assert_eq!(row(&rows(&deps), "app/b.ts")["sites"], 1, "{}", deps.value);
}

/// A hub's sites sum its counted edges' occurrences: over every call hub
/// they are every call witness the ranking admits.
#[test]
fn hub_sites_reconcile_with_witnesses() {
    let dir = weighted();
    let root = dir.0.as_path();
    let calls = |o: &mut QueryOptions| o.edges = vec!["calls".into()];
    let (hubs, _) = all_rows(root, "hubs", calls);
    let (witnesses, _) = all_rows(root, "witnesses", calls);
    assert_eq!(sites(&hubs), witnesses.len() as u64, "{hubs:?}");
    assert_eq!(row(&hubs, "app/a.ts#zeta")["sites"], 3);
    assert_eq!(row(&hubs, "app/a.ts#zeta")["fanIn"], 1);
    // Outgoing hubs count the outgoing edges' sites.
    let (callers, _) = all_rows(root, "hubs", |o| {
        calls(o);
        o.direction = Some("out".into());
    });
    assert_eq!(row(&callers, "app/b.ts#mixed")["sites"], 4, "{callers:?}");
}

/// `rankBy:"pageRank"` weights each edge by its sites: two callees equal by
/// degree rank by the occurrences behind their calls, every node the edges
/// touch is listed, and the ranks sum to 1.
#[test]
fn page_rank_hubs_weight_edges_by_sites() {
    let dir = weighted();
    let root = dir.0.as_path();
    let calls = |o: &mut QueryOptions| o.edges = vec!["calls".into()];
    let (degree, _) = all_rows(root, "hubs", calls);
    let position = |rows: &[Value], id: &str| rows.iter().position(|row| row["id"] == id).unwrap();
    assert_eq!(
        row(&degree, "app/a.ts#zeta")["fanIn"],
        row(&degree, "app/a.ts#alpha")["fanIn"]
    );
    assert!(degree.iter().all(|row| row.get("pageRank").is_none()));
    let (ranked, first) = all_rows(root, "hubs", |o| {
        calls(o);
        o.rank_by = Some("pageRank".into());
    });
    assert_eq!(
        first["rankBy"], "pageRank of calls weighted by sites (confidence high)",
        "{first}"
    );
    assert!(
        position(&ranked, "app/a.ts#zeta") < position(&ranked, "app/a.ts#alpha"),
        "{ranked:?}"
    );
    let ranks = ranked
        .iter()
        .map(|row| row["pageRank"].as_f64().unwrap())
        .collect::<Vec<_>>();
    // A rank is a share, not a measurement: 4 significant digits, so the
    // listed ranks sum to 1 within that rounding.
    for rank in &ranks {
        assert_eq!(
            format!("{rank:.3e}").parse::<f64>().ok(),
            Some(*rank),
            "{ranks:?}"
        );
    }
    let rounding = ranks.iter().map(|rank| rank * 5e-4).sum::<f64>();
    assert!(
        (ranks.iter().sum::<f64>() - 1.0).abs() <= rounding + 1e-12,
        "{ranks:?}"
    );
    assert!(
        ranks.windows(2).all(|pair| pair[0] >= pair[1] - 1e-12),
        "{ranks:?}"
    );
    // A source of edges only (top calls, nothing calls it) is listed too.
    assert_eq!(row(&ranked, "app/c.ts#top")["fanIn"], 0);
    let (_, reversed) = all_rows(root, "hubs", |o| {
        calls(o);
        o.rank_by = Some("pageRank".into());
        o.direction = Some("out".into());
    });
    assert_eq!(
        reversed["rankBy"],
        "pageRank of reversed calls weighted by sites (confidence high)"
    );
    let both = ask(root, "hubs", None, |o| {
        o.rank_by = Some("pageRank".into());
        o.direction = Some("both".into());
    });
    assert_eq!(both.value["errorCode"], "invalidInput", "{}", both.value);
}

/// A symbol ranking without edges ranks calls: symbols import nothing.
#[test]
fn symbol_hubs_default_to_calls() {
    let dir = weighted();
    let root = dir.0.as_path();
    let out = ask(root, "hubs", None, |o| o.kind = Some("symbol".into()));
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    assert_eq!(out.value["rankBy"], "fanIn of calls (confidence high)");
    assert!(rows(&out).iter().all(|row| row.get("symbolName").is_some()));
}

/// `linkage` partitions every call site: `eligible` (with `linked` in it)
/// and `excluded` sum to the calls' `sites`.
#[test]
fn linkage_partitions_every_call_site() {
    let dir = weighted();
    let root = dir.0.as_path();
    let stats = ask(root, "stats", None, |_| {});
    let linkage = &stats.value["linkage"];
    let count = |field: &str| linkage[field].as_u64().unwrap();
    assert!(count("excluded") > 0, "{linkage}");
    assert!(count("linked") <= count("eligible"), "{linkage}");
    assert_eq!(
        Some(count("eligible") + count("excluded")),
        stats.value["calls"]["sites"].as_u64(),
        "{}",
        stats.value
    );
    // The answer states the total itself: a reader reconstructs the
    // partition from `linkage` alone.
    assert_eq!(
        count("sites"),
        count("eligible") + count("excluded"),
        "{linkage}"
    );
}

/// Three calls on one line are three witness rows the default view tells
/// apart: each keeps its own `sourceRange`.
#[test]
fn repeated_sites_on_one_line_stay_distinct_rows() {
    let dir = weighted();
    let root = dir.0.as_path();
    let (witnesses, _) = all_rows(root, "witnesses", |o| {
        o.edges = vec!["calls".into()];
        o.symbol_name = Some("zeta".into());
    });
    assert_eq!(witnesses.len(), 3, "{witnesses:?}");
    let spans = witnesses
        .iter()
        .map(|row| row["sourceRange"].to_string())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(spans.len(), 3, "{witnesses:?}");
    assert!(!spans.contains("null"), "{witnesses:?}");
}

/// A method call on a receiver whose type the parser saw (`let seen =
/// BTreeSet::new()`) and the repository does not declare is a std or
/// foreign method: no declaration here is its target, so it is excluded
/// from the linkage ratio even when a repository type has a method of
/// that name. A receiver of a repository type stays eligible.
#[test]
fn a_foreign_typed_receivers_method_is_outside_the_linkage_ratio() {
    let dir = git_app(&[
        (
            "Cargo.toml",
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        ),
        (
            "src/lib.rs",
            "use std::collections::BTreeSet;\npub struct Bag;\nimpl Bag {\n    pub fn insert(&self, _v: u8) {}\n}\npub fn fill() {\n    let mut seen = BTreeSet::new();\n    seen.insert(1u8);\n}\n",
        ),
    ]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let unresolved = ask(root, "unresolved", Some("src/lib.rs#fill"), |_| {});
    let rows = unresolved.value["rows"].as_array().expect("rows");
    let insert = rows
        .iter()
        .find(|row| row["targetName"] == "seen.insert")
        .unwrap_or_else(|| panic!("{}", unresolved.value));
    assert_eq!(insert["reason"], "not-declared-in-repo", "{insert}");
    let stats = ask(root, "stats", None, |_| {});
    let linkage = &stats.value["linkage"];
    assert_eq!(
        linkage["eligible"].as_u64(),
        linkage["linked"].as_u64(),
        "{linkage}"
    );
}
