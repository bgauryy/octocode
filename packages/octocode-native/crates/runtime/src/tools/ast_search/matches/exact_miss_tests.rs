//! The exact-miss census of a declaration pattern (B81): a note states only
//! what its own count proved, the count and its lead agree, the scan parses
//! only files that spell the pattern's name, and the full census of the kind
//! stays one call away without being run first.
use super::super::scan::DIRECTORY_SCANS;
use crate::tools::ast_search::test_rows::run;
use crate::tools::test_support::{Fixture, checks};
use serde_json::{Value, json};

/// A name no file of this repository spells, this one included.
fn absent_name() -> String {
    ["Qjx", "zkw"].concat()
}

fn exact_notes(out: &Value) -> Vec<&Value> {
    notes(out, "structural.pattern.exact")
}

fn notes<'a>(out: &'a Value, code: &str) -> Vec<&'a Value> {
    out["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|diagnostic| diagnostic["code"] == code)
        .collect()
}

fn message(note: &Value) -> &str {
    note["message"].as_str().expect("message")
}

fn lead_rules(out: &Value) -> Vec<Value> {
    out["related"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|call| call["tool"] == "astSearch")
        .filter_map(|call| call["input"]["queries"][0].get("rule").cloned())
        .collect()
}

fn matched(root: &Fixture, pattern: &str) -> Value {
    run(
        &root.0,
        json!({"operation":"match","path":root.0,"rule":{"pattern":pattern}}),
    )
    .expect("match")
}

/// The directory scans run so far over `root`.
fn scans(root: &Fixture) -> usize {
    let canonical = root.0.canonicalize().expect("canonical");
    DIRECTORY_SCANS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .filter(|scanned| **scanned == canonical)
        .count()
}

#[test]
fn an_unnamed_census_counts_the_nodes_that_contain_the_longest_name() {
    let root = Fixture::new();
    root.write("a.rs", "fn a(c: Config, d: u8) {}\nfn b(x: u8) {}\n");
    root.write("b.rs", "fn c(y: u8) {}\n");
    let out = matched(&root, "fn $N($A: Config) {}");
    assert_eq!(out["matchCount"], 0, "{out}");
    let notes = exact_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = message(notes[0]);
    assert!(
        message.starts_with(
            "1 function_item node(s) contain the text `Config` (the pattern's longest name); in 1 file(s), they differ"
        ),
        "{message}"
    );
    let rules = lead_rules(&out);
    assert_eq!(
        rules,
        [
            json!({"kind": "function_item", "regex": "Config"}),
            json!({"kind": "function_item"})
        ],
        "{out}"
    );
    let lead = &checks::related_call(&out, "astSearch", |input| {
        input["queries"][0]["rule"] == json!({"kind": "function_item", "regex": "Config"})
    })["input"]["queries"][0];
    assert_eq!(
        run(&root.0, lead.clone()).expect("lead")["matchCount"],
        1,
        "the lead lists what the note counts"
    );
}

/// Nothing contains the name: the note says only that, never whether the
/// scope holds nodes of the kind, which it did not count.
#[test]
fn an_unnamed_census_without_the_name_states_only_what_it_counted() {
    let absent = absent_name();
    for (label, source) in [
        ("functions", "fn a(c: u8) {}\n"),
        ("no function", "struct A;\nconst X: u8 = 1;\n"),
    ] {
        let root = Fixture::new();
        root.write("a.rs", source);
        let out = matched(&root, &format!("fn $N($A: {absent}) {{}}"));
        let notes = exact_notes(&out);
        assert_eq!(notes.len(), 1, "{label}: {out}");
        let message = message(notes[0]);
        assert!(
            message.starts_with(&format!(
                "No function_item node in this scope contains the text `{absent}` (the pattern's longest name)."
            )),
            "{label}: {message}"
        );
        for claim in ["holds none", "spells", "node(s) in"] {
            assert!(!message.contains(claim), "{label}: {message}");
        }
        assert_eq!(
            lead_rules(&out),
            [json!({"kind": "function_item"})],
            "{label}: {out}"
        );
    }
}

/// A literal the pattern spells in the wrapper around its declaration (a
/// decorator of an exported class sits in the `export_statement`) is counted
/// there, not in the declaration node that does not hold it.
#[test]
fn a_wrapper_spelled_name_is_counted_in_the_wrapper() {
    let root = Fixture::new();
    root.write(
        "b.ts",
        "@Injectable()\nexport class Svc {\n  run() {}\n}\n\n@Injectable()\nexport class Other extends Base {}\n\nexport class Plain {}\n",
    );
    let out = matched(&root, "@Injectable()\nexport class $C extends Zed { $$$B }");
    assert_eq!(out["matchCount"], 0, "{out}");
    let notes = exact_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let message = message(notes[0]);
    assert!(
        message.starts_with(
            "2 class_declaration node(s) have an export_statement parent that contains the text `Injectable` (the pattern's longest name)"
        ),
        "{message}"
    );
    let census = json!({"kind": "class_declaration", "inside": {"kind": "export_statement", "regex": "Injectable"}});
    assert_eq!(
        lead_rules(&out),
        [
            census.clone(),
            json!({"kind": "class_declaration", "inside": {"kind": "export_statement"}}),
            json!({"kind": "class_declaration"})
        ],
        "{out}"
    );
    let lead = &checks::related_call(&out, "astSearch", |input| {
        input["queries"][0]["rule"] == census
    })["input"]["queries"][0];
    assert_eq!(
        run(&root.0, lead.clone()).expect("lead")["matchCount"],
        2,
        "the lead lists what the note counts"
    );
}

#[test]
fn a_capture_only_census_counts_every_node_of_the_kind() {
    let root = Fixture::new();
    root.write("a.rs", "fn a(c: u8, d: u8) {}\n");
    let out = matched(&root, "fn $N($A: $T) {}");
    let notes = exact_notes(&out);
    let message = message(notes[0]);
    assert!(
        message.starts_with("1 function_item node(s) in 1 file(s)"),
        "{message}"
    );
}

/// A named pattern no declaration of its kind bears: the note says no node
/// of the kind bears the name, and the plain kind rule is offered without
/// running it first, so the miss costs no more scans than an unnamed one.
#[test]
fn a_named_miss_leads_to_its_kind_without_a_kind_census() {
    let root = Fixture::new();
    root.write("a.rs", "fn a(c: Config) {}\nfn b(x: u8) {}\n");
    let name = format!("run_{}", absent_name());
    let before = scans(&root);
    let named = matched(&root, &format!("fn {name}($A: Config) {{}}"));
    let named_scans = scans(&root) - before;
    let before = scans(&root);
    let unnamed = matched(&root, &format!("fn $N($A: {}) {{}}", absent_name()));
    let unnamed_scans = scans(&root) - before;
    assert_eq!(named_scans, unnamed_scans, "{named}\n{unnamed}");
    let notes = exact_notes(&named);
    assert_eq!(notes.len(), 1, "{named}");
    let text = message(notes[0]);
    assert!(
        text.starts_with(&format!(
            "No function_item node in this scope is named `{name}`."
        )),
        "{text}"
    );
    assert!(!text.contains("holds none"), "{text}");
    assert_eq!(
        lead_rules(&named),
        [json!({"kind": "function_item"})],
        "{named}"
    );
    // A scope without the kind: the same note, never a claim it did not count.
    let bare = Fixture::new();
    bare.write("a.rs", "struct A;\n");
    let out = matched(&bare, &format!("fn {name}($A: Config) {{}}"));
    let notes = exact_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    assert!(
        message(notes[0]).starts_with(&format!(
            "No function_item node in this scope is named `{name}`."
        )),
        "{out}"
    );
}

/// Deliberate rule (F6): an empty page whose pattern also has longer
/// spellings carries both notes, the exact miss (why the pattern as written
/// matched none) and the qualified one (which spellings it left out); each
/// keeps its own lead under its own key, none overwritten.
#[test]
fn an_exact_miss_and_a_qualified_note_both_attach_with_their_own_leads() {
    let root = Fixture::new();
    root.write(
        "a.rs",
        "pub trait ToolOutput {}\nstruct A;\nstruct B;\nimpl crate::x::ToolOutput for B {}\nimpl Other for A { fn f() {} }\n",
    );
    let out = matched(&root, "impl ToolOutput for $T {}");
    assert_eq!(out["matchCount"], 0, "{out}");
    assert_eq!(exact_notes(&out).len(), 1, "{out}");
    assert_eq!(
        notes(&out, "structural.pattern.qualified").len(),
        1,
        "{out}"
    );
    assert_eq!(
        lead_rules(&out),
        [
            json!({"kind": "impl_item", "regex": "ToolOutput"}),
            json!({"kind": "impl_item"}),
            json!({"any": [
                {"pattern": "impl ToolOutput for $T {}"},
                {"pattern": "impl $QUALIFIER::ToolOutput for $T {}"}
            ]})
        ],
        "{out}"
    );
}

/// A pattern that spells a wrapper counts only the nodes in that wrapper, so
/// its notes name the wrapper and never claim the whole scope; the kind's
/// nodes in any parent stay one (unrun) call away.
#[test]
fn a_wrapped_pattern_names_its_wrapper_in_every_note() {
    let root = Fixture::new();
    root.write(
        "a.ts",
        "function run(a, b) {}\nfunction other(c: Config) {}\n",
    );
    let wrapped = json!({"kind": "function_declaration", "inside": {"kind": "export_statement"}});
    let any_parent = json!({"kind": "function_declaration"});
    let named = matched(&root, "export function run($A, $B) {}");
    let notes = exact_notes(&named);
    assert_eq!(notes.len(), 1, "{named}");
    assert!(
        message(notes[0]).starts_with(
            "No function_declaration node with an export_statement parent in this scope is named `run`."
        ),
        "{named}"
    );
    assert!(
        !message(notes[0]).contains("every node of the kind"),
        "{named}"
    );
    assert!(
        notes[0]["recovery"]
            .as_str()
            .is_some_and(|r| r.contains("with an export_statement parent")),
        "{named}"
    );
    assert_eq!(
        lead_rules(&named),
        [wrapped.clone(), any_parent.clone()],
        "{named}"
    );
    let census = matched(&root, "export function $N($A, $B) {}");
    let notes = exact_notes(&census);
    assert_eq!(notes.len(), 1, "{census}");
    let text = message(notes[0]);
    assert!(
        text.starts_with(
            "The scope holds no function_declaration node with an export_statement parent;"
        ),
        "{text}"
    );
    assert!(!text.contains("holds none"), "{text}");
    assert_eq!(lead_rules(&census), [any_parent], "{census}");
    assert!(
        checks::find_related_call(&census, "astSearch", |input| {
            input["queries"][0]["operation"] == "symbols"
        })
        .is_none(),
        "a wrapped census proves nothing of the scope's other declarations: {census}"
    );
}

/// A census the scan did not finish proves no absence: a zero note says what
/// was not counted, and a count is a floor.
#[test]
fn a_cut_census_states_what_it_did_not_count() {
    let root = Fixture::new();
    let name = format!("run_{}", absent_name());
    for file in ["a.rs", "b.rs", "c.rs"] {
        root.write(file, format!("struct {name};\n"));
    }
    for pattern in [
        "fn $N($A: $T) {}".to_owned(),
        format!("fn {name}($A: u8) {{}}"),
    ] {
        let out = {
            let _cut = super::census_window(1);
            matched(&root, &pattern)
        };
        assert_eq!(out["matchCount"], 0, "{out}");
        let notes = exact_notes(&out);
        assert_eq!(notes.len(), 1, "{pattern}: {out}");
        let text = message(notes[0]);
        assert!(
            text.contains("the census stopped before it read every file")
                && text.contains("unproven"),
            "{pattern}: {text}"
        );
        assert!(
            !text.contains("holds none") && !text.starts_with("No "),
            "{pattern}: {text}"
        );
    }
    // The same census, not cut, states the absence it proved.
    let out = matched(&root, "fn $N($A: $T) {}");
    assert!(
        exact_notes(&out)
            .first()
            .is_some_and(|note| message(note).contains("holds none")),
        "{out}"
    );
}

/// The `astSearch` listing leads of a row, in key order: a `match` lead by
/// its rule, a `symbols` lead by its operation (the `syntaxTree` view is
/// not a listing).
fn leads(out: &Value) -> Vec<Value> {
    out["related"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|call| call["tool"] == "astSearch")
        .filter(|call| call["input"]["queries"][0]["operation"] != "syntaxTree")
        .map(|call| {
            let query = &call["input"]["queries"][0];
            query
                .get("rule")
                .cloned()
                .unwrap_or_else(|| query["operation"].clone())
        })
        .collect()
}

fn recovery(note: &Value) -> &str {
    note["recovery"].as_str().expect("recovery")
}

/// One arm of [`the_recovery_names_exactly_the_attached_leads`]: its label,
/// scope, pattern, census window, listing leads and the clause naming them.
type LeadCase<'a> = (
    &'a str,
    &'a Fixture,
    &'a str,
    Option<u32>,
    Vec<Value>,
    &'a str,
);

/// A recovery names the leads attached with it, each once and in key order,
/// and no lead that is not attached, in every arm (a census that counted
/// nodes, one that counted none, one cut short).
#[test]
fn the_recovery_names_exactly_the_attached_leads() {
    let wrapped = json!({"kind": "function_declaration", "inside": {"kind": "export_statement"}});
    let any_parent = json!({"kind": "function_declaration"});
    let ts = Fixture::new();
    ts.write("a.ts", "export function run(a, b, c) {}\n");
    let rust = Fixture::new();
    rust.write("a.rs", "fn a(c: Config, d: u8) {}\n");
    let bare = Fixture::new();
    for file in ["a.rs", "b.rs", "c.rs"] {
        bare.write(file, "struct A;\n");
    }
    let decorated = Fixture::new();
    decorated.write("b.ts", "@Injectable()\nexport class Svc extends Base {}\n");
    let unexported = Fixture::new();
    unexported.write("a.ts", "function run(a, b) {}\n");
    let cases: [LeadCase<'_>; 7] = [
        (
            "wrapped name, count > 0",
            &ts,
            "export function run($A, $B) {}",
            None,
            vec![
                json!({"kind": "function_declaration", "inside": {"kind": "export_statement"}, "has": {"kind": "identifier", "regex": "^run$"}}),
                wrapped.clone(),
                any_parent.clone(),
            ],
            "the related calls list these nodes, then every function_declaration node with an export_statement parent, then the kind's nodes in any parent",
        ),
        (
            "wrapped text, count > 0",
            &decorated,
            "@Injectable()\nexport class $C extends Zed { $$$B }",
            None,
            vec![
                json!({"kind": "class_declaration", "inside": {"kind": "export_statement", "regex": "Injectable"}}),
                json!({"kind": "class_declaration", "inside": {"kind": "export_statement"}}),
                json!({"kind": "class_declaration"}),
            ],
            "the related calls list these nodes, then every class_declaration node with an export_statement parent, then the kind's nodes in any parent",
        ),
        (
            "wrapped capture-only, count > 0",
            &ts,
            "export function $N($A, $B) {}",
            None,
            vec![wrapped.clone(), any_parent.clone()],
            "the related calls list these nodes, then the kind's nodes in any parent",
        ),
        (
            "unwrapped text, count > 0",
            &rust,
            "fn $N($A: Config) {}",
            None,
            vec![
                json!({"kind": "function_item", "regex": "Config"}),
                json!({"kind": "function_item"}),
            ],
            "the related calls list these nodes, then every node of the kind",
        ),
        (
            "wrapped name, zero",
            &unexported,
            "export function run($A, $B) {}",
            None,
            vec![wrapped.clone(), any_parent.clone()],
            "the related calls list every function_declaration node with an export_statement parent, then the kind's nodes in any parent",
        ),
        (
            "unwrapped capture-only, zero",
            &bare,
            "fn $N($A: $T) {}",
            None,
            vec![json!("symbols")],
            "the related call lists what the scope declares",
        ),
        (
            "unwrapped capture-only, cut zero",
            &bare,
            "fn $N($A: $T) {}",
            Some(1),
            vec![json!("symbols")],
            "the related call lists what the scope declares",
        ),
    ];
    for (label, root, pattern, window, expected, named) in cases {
        let out = {
            let _cut = window.map(super::census_window);
            matched(root, pattern)
        };
        assert_eq!(out["matchCount"], 0, "{label}: {out}");
        let notes = exact_notes(&out);
        assert_eq!(notes.len(), 1, "{label}: {out}");
        assert_eq!(leads(&out), expected, "{label}: {out}");
        // A lead clause may open the sentence, capitalized.
        let recovery = recovery(notes[0]).to_lowercase();
        assert!(recovery.contains(named), "{label}: {recovery}");
        assert_eq!(
            recovery.matches("the related call").count(),
            1,
            "{label}: one sentence names the leads: {recovery}"
        );
        for claim in ["broadenSearch lists", "and the next lists"] {
            assert!(!recovery.contains(claim), "{label}: {recovery}");
        }
    }
}

/// A cut census that counted nodes states its count as a floor, never an
/// absence.
#[test]
fn a_cut_census_with_a_count_states_a_floor() {
    let root = Fixture::new();
    for file in ["a.rs", "b.rs", "c.rs"] {
        root.write(file, "fn a(c: u8, d: u8) {}\n");
    }
    let out = {
        let _cut = super::census_window(1);
        matched(&root, "fn $N($A: $T) {}")
    };
    let notes = exact_notes(&out);
    assert_eq!(notes.len(), 1, "{out}");
    let text = message(notes[0]);
    assert!(
        text.contains("node(s) in")
            && text.contains("the census stopped before it read every file")
            && text.ends_with("so the count is a floor."),
        "{text}"
    );
    assert!(!text.contains("unproven"), "{text}");
}

/// Each part of a census it could not read (a file it could not open, one
/// over the parse cap, a directory the walk could not list) leaves its zero
/// unproven, named for what it missed, for a full and an anchored census.
#[cfg(unix)]
#[test]
fn a_census_that_missed_a_file_or_entry_proves_no_absence() {
    use std::os::unix::fs::PermissionsExt;
    let name = format!("run_{}", absent_name());
    let locked = |path: &std::path::Path| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000)).expect("chmod");
    };
    let restore = |path: &std::path::Path, mode| {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).expect("restore");
    };
    // A file it could not open, under a full and an anchored census.
    let root = Fixture::new();
    root.write("a.rs", "struct A;\n");
    root.write("locked.rs", format!("fn {name}(a: u8, b: u8) {{}}\n"));
    let file = root.0.join("locked.rs");
    locked(&file);
    let readable = std::fs::read(&file).is_ok();
    let outs = [
        "fn $N($A: $T) {}".to_owned(),
        format!("fn {name}($A: u8) {{}}"),
    ]
    .map(|pattern| (matched(&root, &pattern), pattern));
    restore(&file, 0o644);
    if !readable {
        for (out, pattern) in &outs {
            let notes = exact_notes(out);
            assert_eq!(notes.len(), 1, "{pattern}: {out}");
            let text = message(notes[0]);
            assert!(
                text.contains("1 file(s) were unreadable or too large to parse")
                    && text.contains("unproven"),
                "{pattern}: {text}"
            );
        }
    }
    // A file over the parse cap, sized without writing its bytes.
    let root = Fixture::new();
    root.write("a.rs", "struct A;\n");
    std::fs::File::create(root.0.join("big.rs"))
        .and_then(|big| big.set_len(octocode_engine::security::MAX_CONTENT_SIZE as u64 + 1))
        .expect("big");
    let out = matched(&root, "fn $N($A: $T) {}");
    let text = message(exact_notes(&out)[0]);
    assert!(
        text.contains("1 file(s) were unreadable or too large to parse")
            && text.contains("unproven"),
        "{text}"
    );
    // A directory the walk could not list.
    let root = Fixture::new();
    root.write("a.rs", "struct A;\n");
    root.write("locked/b.rs", "fn b(a: u8, b: u8) {}\n");
    let dir = root.0.join("locked");
    locked(&dir);
    let listable = std::fs::read_dir(&dir).is_ok();
    let out = matched(&root, "fn $N($A: $T) {}");
    restore(&dir, 0o755);
    if !listable {
        let text = message(exact_notes(&out)[0]);
        assert!(
            text.contains("1 entry(ies) could not be read") && text.contains("unproven"),
            "{text}"
        );
    }
}
