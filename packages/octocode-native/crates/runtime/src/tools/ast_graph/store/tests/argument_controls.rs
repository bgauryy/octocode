use super::*;

#[test]
fn recovered_and_dynamic_scope_arguments_do_not_create_bindings() {
    let source = "function handler(){} function broken(){ const missing; consume(handler); } function healthy(){ consume(handler); }";
    let dir = git_app(&[
        ("scope.js", source),
        (
            "dynamic.js",
            "function handler(){} function dynamic(){ eval(source); consume(handler); }",
        ),
    ]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    for (target, reason) in [
        ("scope.js#broken", "recovered-occurrence"),
        ("dynamic.js#dynamic", "scope-not-assessed"),
    ] {
        let out = ask(&dir.0, "unresolved", Some(target), |o| {
            o.edges = vec!["passes".into()];
        });
        let row = out.value["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["targetName"] == "handler")
            .unwrap_or_else(|| panic!("missing handler: {}; build {}", out.value, built.value));
        assert_eq!(row["reason"], reason, "{}", out.value);
        assert!(row.get("candidates").is_none());
        let linked = ask(&dir.0, "witnesses", Some(target), |o| {
            o.edges = vec!["passes".into()];
        });
        assert_eq!(linked.value["total"], 0, "{}", linked.value);
    }
    let healthy = ask(&dir.0, "witnesses", Some("scope.js#healthy"), |o| {
        o.edges = vec!["passes".into()];
    });
    assert_eq!(healthy.value["total"], 1, "{}", healthy.value);
    assert!(
        built.value["argumentCoverage"]["byLanguage"]["javascript"]
            .get("counts")
            .is_none()
    );
}

#[test]
fn argument_binding_premises_exclude_unrelated_members() {
    let dir = git_app(&[
        ("exported.ts", "export function handler(){}"),
        (
            "imported.ts",
            "import {handler} from './exported'; class Box { handler(){} } export function run(){ sink(handler); }",
        ),
        (
            "scope.rs",
            "fn handler(){} const VALUE:i32=1; struct Box; impl Box { fn handler(){} const VALUE:i32=2; fn inside(){consume(handler, VALUE);} } fn outside(){ consume(handler); }",
        ),
    ]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let imported = ask(&dir.0, "witnesses", Some("imported.ts#run"), |o| {
        o.edges = vec!["passes".into()];
    });
    assert_eq!(imported.value["total"], 1, "{}", imported.value);
    assert_eq!(imported.value["results"][0]["to"], "exported.ts#handler");
    assert_eq!(imported.value["results"][0]["detail"], "import-binding");
    let rust = ask(&dir.0, "witnesses", Some("scope.rs"), |o| {
        o.edges = vec!["passes".into()];
    });
    assert_eq!(rust.value["total"], 3, "{}", rust.value);
    for row in rust.value["results"].as_array().unwrap() {
        assert!(
            matches!(
                row["to"].as_str(),
                Some("scope.rs#handler" | "scope.rs#VALUE")
            ),
            "{row}"
        );
    }
}

#[test]
fn nearest_ordinary_lexical_scope_wins_for_argument_candidates() {
    let dir = git_app(&[(
        "module.rs",
        "fn handler(){} mod inner { fn handler(){} fn run(){consume(handler);} } fn outer(){ consume(handler); }",
    )]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let rows = ask(&dir.0, "witnesses", Some("module.rs"), |o| {
        o.edges = vec!["passes".into()];
    });
    assert_eq!(rows.value["total"], 2, "{}", rows.value);
    for row in rows.value["results"].as_array().unwrap() {
        if row["from"] == "module.rs#outer" {
            assert_eq!(row["to"], "module.rs#handler");
        } else {
            assert_eq!(row["to"], "module.rs#inner.handler", "{row}");
        }
    }
}
