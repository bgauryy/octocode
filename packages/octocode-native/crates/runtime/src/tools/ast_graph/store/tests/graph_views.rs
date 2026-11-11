//! Directory targets, `view:"modules"`, not-found leads that stay under the
//! reference's directory, and the witnesses' confidence-filter count.
use super::*;

fn modules_app() -> Fixture {
    git_app(&[
        (
            "main.ts",
            "import { load } from './config/load';\nimport { fmt } from './util/fmt';\nexport function main() { return fmt(load()); }\nexport function run(x: any) { return x.close(); }\n",
        ),
        (
            "config/load.ts",
            "import { fmt } from '../util/fmt';\nimport { schema } from './schema';\nexport function load() { return fmt(schema()); }\n",
        ),
        (
            "config/schema.ts",
            "export function schema() { return 'x'; }\n",
        ),
        (
            "util/fmt.ts",
            "export function fmt(v: string) { return v; }\n",
        ),
        (
            "util/box.ts",
            "export class Box { close() { return 1; } }\n",
        ),
        ("weak/a.ts", "export function target() {}\n"),
        ("weak/b.ts", "export function target() {}\n"),
        (
            "weak/barrel.ts",
            "export * from './a'; export * from './b';\n",
        ),
        (
            "weak/use.ts",
            "import { target } from './barrel';\nexport function go() { return target(); }\n",
        ),
    ])
}

fn built() -> Fixture {
    let dir = modules_app();
    let out = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    dir
}

fn sorted(mut values: Vec<String>) -> Vec<String> {
    values.sort();
    values
}

/// `deps`/`dependents` on a directory walk from every file under it: the
/// rows are the usual file pairs, never files inside the directory itself.
#[test]
fn directory_target_walks_every_file_under_it() {
    let dir = built();
    let root = dir.0.as_path();
    let absolute = root.join("app/config").to_string_lossy().into_owned();
    for target in ["config", "config/", absolute.as_str()] {
        let deps = ask(root, "deps", Some(target), |_| {});
        assert_eq!(deps.class, ExitClass::Success, "{target}: {}", deps.value);
        assert_eq!(ids(&deps), vec!["util/fmt.ts"], "{target}: {}", deps.value);
        assert_eq!(deps.value["rows"][0]["from"], "config/load.ts");
        assert_eq!(
            deps.value["target"],
            json!({"id": "config", "kind": "directory"}),
            "{}",
            deps.value
        );
    }
    let dependents = ask(root, "dependents", Some("util"), |_| {});
    assert_eq!(
        sorted(ids(&dependents)),
        vec!["config/load.ts", "main.ts"],
        "{}",
        dependents.value
    );
}

/// `view:"modules"`: one row per (importing directory, imported directory)
/// pair with its edge count, in key order; counts sum to the edge total.
#[test]
fn modules_view_counts_edges_per_directory_pair() {
    let dir = built();
    let root = dir.0.as_path();
    let modules =
        |op: &str, target: &str| ask(root, op, Some(target), |o| o.view = Some("modules".into()));
    let deps = modules("deps", "main.ts");
    assert_eq!(
        deps.value["rows"],
        json!([
            {"from": ".", "to": "config", "edges": 1},
            {"from": ".", "to": "util", "edges": 1},
        ]),
        "{}",
        deps.value
    );
    let dependents = modules("dependents", "util");
    assert_eq!(
        dependents.value["rows"],
        json!([
            {"from": ".", "to": "util", "edges": 1},
            {"from": "config", "to": "util", "edges": 1},
        ]),
        "{}",
        dependents.value
    );
    let callers = modules("callers", "util/fmt.ts#fmt");
    assert_eq!(
        callers.value["rows"],
        json!([
            {"from": ".", "to": "util", "edges": 1},
            {"from": "config", "to": "util", "edges": 1},
        ]),
        "{}",
        callers.value
    );
}

/// Walking the modules view one row per page reconstructs the one-page
/// answer exactly once, through the emitted offsets.
#[test]
fn modules_view_pages_reconstruct_every_pair_once() {
    let dir = built();
    let root = dir.0.as_path();
    let whole = ask(root, "dependents", Some("util"), |o| {
        o.view = Some("modules".into());
    });
    let mut walked = Vec::new();
    let mut offset = Some(0);
    while let Some(at) = offset {
        let page = ask(root, "dependents", Some("util"), |o| {
            o.view = Some("modules".into());
            o.limit = Some(1);
            o.offset = Some(at);
        });
        walked.extend(page.value["rows"].as_array().unwrap().iter().cloned());
        offset = page
            .next
            .iter()
            .find_map(|(channel, resume)| match (channel, resume) {
                (Channel::NextPage, Resume::Row { offset, .. }) => Some(*offset),
                _ => None,
            });
    }
    assert_eq!(Value::Array(walked), whole.value["rows"]);
}

/// A path-shaped reference that names nothing suggests only nodes under its
/// deepest directory the snapshot holds that share its name, never
/// same-word nodes elsewhere and never the directory's other nodes; its
/// `find` lead looks for the name everywhere.
#[test]
fn not_found_path_suggests_only_nodes_under_its_directory() {
    let dir = built();
    let out = ask(dir.0.as_path(), "deps", Some("config/schem"), |_| {});
    assert_eq!(out.value["errorCode"], "notFound", "{}", out.value);
    let suggestions = out.value["suggestions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(!suggestions.is_empty(), "{}", out.value);
    for key in &suggestions {
        let key = key.as_str().unwrap();
        assert!(
            key.starts_with("config/schema.ts"),
            "{key} is outside config/ or does not share the name: {}",
            out.value
        );
    }
    // `fmt` lives in util/: nothing under config/ shares the name, so
    // nothing is suggested, and the find lead names it.
    let out = ask(dir.0.as_path(), "deps", Some("config/fmt"), |_| {});
    assert_eq!(out.value["errorCode"], "notFound", "{}", out.value);
    assert!(out.value.get("suggestions").is_none(), "{}", out.value);
    assert_eq!(out.value["find"], "fmt", "{}", out.value);
}

/// The first symbol row of a `find`, `symbols` or `hubs` page carries a
/// runnable lspSearch lead at its declaration, its path workspace-relative
/// (the snapshot's root is `app`, below the workspace); the other symbol
/// rows carry none, since each rebuilds the same call from its own `file`,
/// `name` and `line`. Edge rows (`callers`) verify their edge instead: see
/// the test after this one.
#[test]
fn symbol_rows_carry_a_workspace_relative_lsp_lead() {
    let dir = built();
    let root = dir.0.as_path();
    let symbols = ask(root, "symbols", Some("util/fmt.ts"), |_| {});
    assert_eq!(
        symbols.value["rows"][0]["verify"],
        json!([{"kind": "related", "tool": "lspSearch", "input": {"queries": [
            {"operation": "references", "path": "app/util/fmt.ts", "symbolName": "fmt", "line": 1}
        ]}}]),
        "{}",
        symbols.value
    );
    let answers = [
        ask(root, "find", Some("fmt"), |o| {
            o.kind = Some("symbol".into());
        }),
        ask(root, "symbols", Some("util/box.ts"), |_| {}),
        ask(root, "hubs", None, |o| o.edges = vec!["calls".into()]),
    ];
    for out in &answers {
        let rows = out.value["rows"].as_array().unwrap();
        let mut symbols = rows.iter().filter(|row| row["kind"] == "symbol");
        let row = symbols
            .next()
            .unwrap_or_else(|| panic!("no symbol row: {}", out.value));
        let lead = &row["verify"][0]["input"]["queries"][0];
        let path = lead["path"]
            .as_str()
            .unwrap_or_else(|| panic!("no lead: {row}"));
        assert!(root.join(path).is_file(), "{path} does not exist: {row}");
        // The lead is the row's own fields: the snapshot root, then `file`.
        assert_eq!(
            path,
            format!("app/{}", row["file"].as_str().unwrap()),
            "{row}"
        );
        let name = lead["symbolName"].as_str().unwrap();
        assert!(
            row["name"].as_str().unwrap().ends_with(name),
            "{name}: {row}"
        );
        assert_eq!(lead["line"], row["line"], "{row}");
        for other in symbols {
            assert!(other.get("verify").is_none(), "{other}");
        }
    }
}

/// An edge row's `verify` confirms the edge, not the row's own symbol:
/// lspSearch references of the symbol the edge points to, at its
/// declaration. The edge holds when one of those references lies at the
/// row's `edgeLine` inside the calling symbol: a `callers` row's own span, a
/// `callees` row's `from`.
#[test]
fn an_edge_row_verifies_its_edge_by_the_references_of_its_target() {
    let dir = built();
    let root = dir.0.as_path();
    let lead = |row: &Value| row["verify"][0]["input"]["queries"][0].clone();
    let declared = |path: &str, name: &str, line: u64| json!({"operation": "references", "path": path, "symbolName": name, "line": line});
    // Callers of `fmt`: every row's lead is the references of `fmt`, and
    // the row's span holds the call.
    let callers = ask(root, "callers", Some("util/fmt.ts#fmt"), |_| {});
    let rows = callers.value["rows"].as_array().unwrap();
    assert!(rows.len() >= 2, "{}", callers.value);
    for row in rows {
        assert_eq!(lead(row), declared("app/util/fmt.ts", "fmt", 1), "{row}");
        let (line, edge) = (
            row["line"].as_u64().unwrap(),
            row["edgeLine"].as_u64().unwrap(),
        );
        let end = row["endLine"].as_u64().unwrap_or(line);
        assert!(
            (line..=end).contains(&edge),
            "the call lies in the caller: {row}"
        );
    }
    // Callees of `main`: each row is the edge's target, so its lead is the
    // references of the row's own symbol.
    let callees = ask(root, "callees", Some("main.ts#main"), |_| {});
    let rows = callees.value["rows"].as_array().unwrap();
    assert!(!rows.is_empty(), "{}", callees.value);
    for row in rows {
        let file = row["file"].as_str().unwrap();
        let expected = declared(
            &format!("app/{file}"),
            row["name"].as_str().unwrap(),
            row["line"].as_u64().unwrap(),
        );
        assert_eq!(lead(row), expected, "{row}");
    }
    // Two levels of callers of `schema`: `main` reaches it through `load`,
    // so `main`'s edge is `main -> load`, verified by `load`'s references.
    let walk = ask(root, "callers", Some("config/schema.ts#schema"), |o| {
        o.depth = Some(2);
    });
    let main = walk.value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "main.ts#main")
        .unwrap_or_else(|| panic!("main at depth 2: {}", walk.value));
    assert_eq!(main["from"], "config/load.ts#load", "{main}");
    assert_eq!(
        lead(main),
        declared("app/config/load.ts", "load", 3),
        "{main}"
    );
}

/// Two sub-modules importing one shared file, a `config.ts` beside a
/// `config/` and a `configx/`, 120 importers of one file, and a directory
/// whose name holds `#`.
fn views_app() -> Fixture {
    let mut files = vec![
        ("shared/s.ts".to_owned(), "export const s = 1;\n".to_owned()),
        (
            "lib/fmt.ts".to_owned(),
            "export function fmt() {}\n".to_owned(),
        ),
        (
            "config.ts".to_owned(),
            "import { s } from './shared/s';\nexport const c = s;\n".to_owned(),
        ),
        (
            "config/a.ts".to_owned(),
            "import { fmt } from '../lib/fmt';\nexport const a = fmt;\n".to_owned(),
        ),
        (
            "configx/z.ts".to_owned(),
            "import { s } from '../shared/s';\nexport const z = s;\n".to_owned(),
        ),
        (
            "odd#dir/w.ts".to_owned(),
            "export function hashed() {}\n".to_owned(),
        ),
        ("x.ts".to_owned(), "export const x = 1;\n".to_owned()),
        (
            "x.ts#y.ts".to_owned(),
            "export function hashed() {}\n".to_owned(),
        ),
    ];
    for (dir, name) in [("group/p", "a"), ("group/q", "b"), ("group/q", "c")] {
        files.push((
            format!("{dir}/{name}.ts"),
            format!("import {{ s }} from '../../shared/s';\nexport const {name} = s;\n"),
        ));
    }
    for i in 0..120 {
        files.push((
            format!("big/f{i}.ts"),
            "import { fmt } from '../lib/fmt';\nexport const f = fmt;\n".to_owned(),
        ));
    }
    let files = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect::<Vec<_>>();
    let dir = git_app(&files);
    let out = ingest_fixture(dir.0.as_path(), None);
    assert_eq!(out.class, ExitClass::Success, "{}", out.value);
    dir
}

/// Every grouped row of one query, over all its pages.
fn grouped(root: &Path, op: &str, target: &str, view: &str) -> Value {
    let mut rows = Vec::new();
    let mut offset = Some(0);
    while let Some(at) = offset {
        let out = ask(root, op, Some(target), |o| {
            o.view = Some(view.into());
            o.offset = Some(at);
        });
        assert_eq!(
            out.class,
            ExitClass::Success,
            "{op} {target}: {}",
            out.value
        );
        rows.extend(out.value["rows"].as_array().unwrap().iter().cloned());
        offset = out
            .next
            .iter()
            .find_map(|(channel, resume)| match (channel, resume) {
                (Channel::NextPage, Resume::Row { offset, .. }) => Some(*offset),
                _ => None,
            });
    }
    Value::Array(rows)
}

/// The grouped views count every walked edge, also edges into a file another
/// edge reached first: `deps` of the importers and `dependents` of the
/// imported file state the same pairs.
#[test]
fn grouped_views_count_every_edge_into_a_shared_file() {
    let dir = views_app();
    let root = dir.0.as_path();
    let pairs = json!([
        {"from": "group/p", "to": "shared", "edges": 1},
        {"from": "group/q", "to": "shared", "edges": 2},
    ]);
    assert_eq!(grouped(root, "deps", "group", "modules"), pairs);
    assert_eq!(
        grouped(root, "dependents", "shared", "modules"),
        json!([
            {"from": ".", "to": "shared", "edges": 1},
            {"from": "configx", "to": "shared", "edges": 1},
            {"from": "group/p", "to": "shared", "edges": 1},
            {"from": "group/q", "to": "shared", "edges": 2},
        ])
    );
    let big = json!([{"from": "big", "to": "lib", "edges": 120}]);
    assert_eq!(grouped(root, "deps", "big", "modules"), big);
    assert_eq!(
        grouped(root, "dependents", "lib", "modules"),
        json!([
            {"from": "big", "to": "lib", "edges": 120},
            {"from": "config", "to": "lib", "edges": 1},
        ])
    );
    assert_eq!(
        grouped(root, "deps", "group", "files"),
        json!([{"file": "shared/s.ts", "edges": 3}])
    );
    assert_eq!(
        grouped(root, "deps", "big", "files"),
        json!([{"file": "lib/fmt.ts", "edges": 120}])
    );
    // One query's views count the same walked edges.
    let sum = |rows: Value| -> u64 {
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| row["edges"].as_u64().unwrap())
            .sum()
    };
    for (op, target) in [
        ("deps", "group"),
        ("dependents", "shared"),
        ("dependents", "lib"),
        ("deps", "big"),
    ] {
        let modules = sum(grouped(root, op, target, "modules"));
        assert!(modules > 0, "{op} {target}");
        assert_eq!(
            modules,
            sum(grouped(root, op, target, "files")),
            "{op} {target}"
        );
    }
}

/// A directory target takes only files under it: never `config.ts` beside
/// it or `configx/` sharing its name's start.
#[test]
fn directory_target_skips_sibling_prefixes() {
    let dir = views_app();
    let deps = ask(dir.0.as_path(), "deps", Some("config"), |_| {});
    assert_eq!(ids(&deps), vec!["lib/fmt.ts"], "{}", deps.value);
}

/// `.` and the root's absolute path are the root directory; `/` names
/// nothing.
#[test]
fn dot_is_the_root_and_slash_is_not_found() {
    let dir = views_app();
    let root = dir.0.as_path();
    let absolute = root.join("app").to_string_lossy().into_owned();
    for target in [".", absolute.as_str()] {
        let out = ask(root, "deps", Some(target), |_| {});
        assert_eq!(
            out.value["target"],
            json!({"id": ".", "kind": "directory"}),
            "{target}: {}",
            out.value
        );
        assert!(
            ids(&out).is_empty(),
            "inner edges stay unlisted: {}",
            out.value
        );
    }
    let slash = ask(root, "deps", Some("/"), |_| {});
    assert_eq!(slash.value["errorCode"], "notFound", "{}", slash.value);
}

/// A reference that is empty once trimmed (`./`, blanks) or only slashes
/// names nothing, also where the workspace is the snapshot root, and gets
/// no suggestions.
#[test]
fn empty_references_are_not_found_at_the_workspace_root() {
    let dir = Fixture::new();
    std::fs::create_dir_all(dir.0.as_path().join(".git")).expect("git");
    write_under(
        &dir,
        "",
        &[
            (
                "a.ts",
                "import { b } from './lib/b';\nexport const a = b;\n",
            ),
            ("lib/b.ts", "export const b = 1;\n"),
        ],
    );
    let root = dir.0.as_path();
    let built = ingest_root(root, root, None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let dot = ask(root, "deps", Some("."), |_| {});
    assert_eq!(dot.value["target"]["id"], ".", "{}", dot.value);
    for target in ["./", " ", "/", "//"] {
        let out = ask(root, "deps", Some(target), |_| {});
        assert_eq!(
            out.value["errorCode"], "notFound",
            "{target:?}: {}",
            out.value
        );
        assert!(
            out.value.get("suggestions").is_none(),
            "{target:?}: {}",
            out.value
        );
    }
}

/// A directory spelled relative to the workspace, above the snapshot root,
/// is the same directory.
#[test]
fn workspace_relative_directory_target() {
    let dir = views_app();
    let deps = ask(dir.0.as_path(), "deps", Some("app/config"), |_| {});
    assert_eq!(ids(&deps), vec!["lib/fmt.ts"], "{}", deps.value);
    assert_eq!(deps.value["target"]["id"], "config", "{}", deps.value);
}

/// Only `deps` and `dependents` walk a directory; `callers` names nodes.
#[test]
fn callers_take_no_directory_target() {
    let dir = views_app();
    let out = ask(dir.0.as_path(), "callers", Some("lib"), |_| {});
    assert_eq!(out.value["errorCode"], "notFound", "{}", out.value);
}

/// An absolute path that names nothing gets the leads of its root-relative
/// form.
#[test]
fn absolute_miss_suggests_like_its_relative_form() {
    let dir = built();
    let root = dir.0.as_path();
    let suggestions = |target: &str| {
        let out = ask(root, "deps", Some(target), |_| {});
        assert_eq!(out.value["errorCode"], "notFound", "{}", out.value);
        out.value["suggestions"].clone()
    };
    let relative = suggestions("config/schem");
    assert!(relative.as_array().is_some_and(|keys| !keys.is_empty()));
    let absolute = root.join("app/config/schem").to_string_lossy().into_owned();
    assert_eq!(suggestions(&absolute), relative);
}

/// A symbol in a file whose path holds `#` keeps its whole file path in its
/// lead and in `view:"files"`.
#[test]
fn hash_in_a_file_path_keeps_the_whole_path() {
    let dir = views_app();
    let root = dir.0.as_path();
    let lead = |file: &str| {
        let out = ask(root, "symbols", Some(file), |_| {});
        out.value["rows"][0]["verify"][0]["input"]["queries"][0]["path"].clone()
    };
    assert_eq!(lead("odd#dir/w.ts"), "app/odd#dir/w.ts");
    // `x.ts#y.ts#hashed` lies in `x.ts#y.ts`, not in the file `x.ts`.
    assert_eq!(lead("x.ts#y.ts"), "app/x.ts#y.ts");
}

/// `witnesses` states how many matching call witnesses the confidence
/// filter left out, and omits the count when it drops none.
#[test]
fn witnesses_count_calls_filtered_by_confidence() {
    let dir = built();
    let root = dir.0.as_path();
    let at = |confidence: &str| {
        ask(root, "witnesses", None, |o| {
            o.edges = vec!["calls".into()];
            o.confidence = Some(confidence.into());
        })
    };
    let (low, high) = (at("low"), at("high"));
    let total = |out: &GraphOutput| out.value["pagination"]["totalItems"].as_u64().unwrap();
    assert!(
        total(&low) > total(&high),
        "fixture lacks weak calls: {}",
        low.value
    );
    assert_eq!(
        high.value["filteredByConfidence"],
        json!(total(&low) - total(&high)),
        "{}",
        high.value
    );
    assert!(
        low.value.get("filteredByConfidence").is_none(),
        "{}",
        low.value
    );
}

/// Every edge kind the graph stores answers with rows that follow one lead
/// rule: the first symbol row of a page carries a runnable lspSearch lead
/// (the others carry their edge's, or rebuild it from their own fields), a
/// file or package row carries none. The test walks `EdgeKind::ALL`, so a
/// new kind must appear in the fixture and obey the rule.
#[test]
fn every_edge_kind_leads_its_symbol_rows_and_only_them() {
    let dir = git_app(&[
        (
            "lib.ts",
            "export class Box {}\nexport function fmt() {}\nexport function take(f: unknown) { return f; }\n",
        ),
        (
            "app.ts",
            "import { Box, fmt, take } from './lib';\nimport path from 'node:path';\nexport class Crate extends Box {}\nexport function main() { fmt(); take(fmt); return path; }\n",
        ),
    ]);
    let root = dir.0.as_path();
    let built = ingest_fixture(root, None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    for kind in super::super::format::EdgeKind::ALL {
        let name = kind.as_str();
        let mut listed = 0;
        for target in ["app.ts", "app.ts#main", "app.ts#Crate", "lib.ts"] {
            let out = ask(root, "deps", Some(target), |o| {
                o.edges = vec![name.into()];
            });
            let rows = out.value["rows"].as_array().cloned().unwrap_or_default();
            listed += rows.len();
            let lead = |row: &Value| row["verify"][0]["tool"].as_str().map(str::to_owned);
            if let Some(first) = rows.iter().find(|row| row["kind"] == "symbol") {
                assert_eq!(
                    lead(first).as_deref(),
                    Some("lspSearch"),
                    "{name} first symbol row has no lead: {first}"
                );
            }
            for row in &rows {
                assert_eq!(row["edge"], name, "{row}");
                if row["kind"] == "symbol" {
                    assert!(
                        lead(row).is_none_or(|tool| tool == "lspSearch"),
                        "{name}: {row}"
                    );
                } else {
                    assert_eq!(lead(row), None, "{name} non-symbol row has a lead: {row}");
                }
            }
        }
        assert!(listed > 0, "no {name} row in the fixture");
    }
}

/// The stats summary's calls lead is the most-called ranking: run as
/// offered, it ranks symbols by distinct production callers, and the details
/// row states what that ranking read.
#[test]
fn stats_calls_lead_ranks_the_most_called_symbols() {
    let dir = built();
    let root = dir.0.as_path();
    let stats = ask(root, "stats", None, |_| {});
    let lead = stats.value["related"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|lead| {
            lead["input"]["queries"]
                .as_array()
                .cloned()
                .unwrap_or_default()
        })
        .find(|row| row["operation"] == "hubs" && row["edges"] == json!(["calls"]))
        .unwrap_or_else(|| panic!("no calls hubs lead: {}", stats.value));
    let hubs = ask(root, "hubs", None, |o| {
        o.edges = vec!["calls".into()];
        o.graph = lead["graph"].as_str().map(str::to_owned);
    });
    assert_eq!(hubs.class, ExitClass::Success, "{}", hubs.value);
    let top = &hubs.value["rows"][0];
    assert_eq!(top["id"], "util/fmt.ts#fmt", "{}", hubs.value);
    assert_eq!(top["fanIn"], 2, "{}", hubs.value);
    assert!(
        hubs.value["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["kind"] == "symbol"),
        "{}",
        hubs.value
    );
    let details = ask(root, "stats", None, |o| o.view = Some("details".into()));
    assert_eq!(details.value["callConfidence"], "high", "{}", details.value);
    assert_eq!(details.value["testEdges"], "excluded", "{}", details.value);
}

/// A `hubs` ranking states what it counts once, on its first page, in
/// `rankBy` (a fresh request's `edges` and defaulted `confidence` are echoes
/// the runtime drops); a later page carries rows only.
#[test]
fn hubs_state_their_ranking_once_on_the_first_page() {
    let dir = built();
    let root = dir.0.as_path();
    let first = ask(root, "hubs", None, |o| {
        o.edges = vec!["calls".into()];
        o.limit = Some(1);
    });
    assert_eq!(first.class, ExitClass::Success, "{}", first.value);
    assert_eq!(
        first.value["rankBy"], "fanIn of calls (confidence high)",
        "{}",
        first.value
    );
    assert_eq!(first.value["testEdges"], "excluded", "{}", first.value);
    for duplicate in ["edges", "confidence"] {
        assert!(first.value.get(duplicate).is_none(), "{}", first.value);
    }
    let imports = ask(root, "hubs", None, |o| o.direction = Some("out".into()));
    assert_eq!(
        imports.value["rankBy"], "fanOut of imports",
        "{}",
        imports.value
    );
    let later = ask(root, "hubs", None, |o| {
        o.edges = vec!["calls".into()];
        o.limit = Some(1);
        o.offset = Some(1);
    });
    assert_eq!(later.class, ExitClass::Success, "{}", later.value);
    assert!(
        later.value["rows"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()),
        "{}",
        later.value
    );
    for header in ["rankBy", "testEdges", "edges", "confidence"] {
        assert!(later.value.get(header).is_none(), "{}", later.value);
    }
}
