use super::*;

fn slice<'a>(source: &'a str, range: &Value) -> &'a str {
    &source
        [range["startByte"].as_u64().unwrap() as usize..range["endByte"].as_u64().unwrap() as usize]
}

#[test]
fn named_argument_occurrences_page_without_becoming_invocations() {
    let source = "function handler() {}\nfunction sink(...values: unknown[]) {}\nexport function run(){ sink(handler, handler); registry[key](handler); }\n";
    let dir = git_app(&[("args.ts", source)]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let mut ids = BTreeSet::new();
    let mut slots = BTreeSet::new();
    for offset in 0..3 {
        let out = ask(&dir.0, "witnesses", Some("args.ts#run"), |o| {
            o.edges = vec!["passes".into()];
            o.to = Some("args.ts#handler".into());
            o.limit = Some(1);
            o.offset = Some(offset);
            o.confidence = Some("high".into());
        });
        assert_eq!(out.value["total"], 3, "{}", out.value);
        let row = &out.value["results"][0];
        assert_eq!(row["edge"], "passes");
        assert_eq!(row["targetName"], "handler");
        assert_eq!(row["from"], "args.ts#run");
        assert_eq!(slice(source, &row["sourceRange"]), "handler");
        assert_eq!(slice(source, &row["selectionByteRange"]), "handler");
        let recipient = slice(source, &row["recipient"]["sourceRange"]);
        assert!(
            matches!(
                recipient,
                "sink(handler, handler)" | "registry[key](handler)"
            ),
            "{recipient}"
        );
        assert_eq!(
            row["sourceDigest"],
            octocode_engine::digest::sha256(source.as_bytes())
        );
        assert_eq!(row["parseAssessment"]["scope"], "occurrence");
        assert!(ids.insert(row["id"].as_str().unwrap().to_owned()));
        assert!(
            slots.insert((
                row["recipient"]["occurrenceId"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                row["argument"]["groupIndex"].as_u64().unwrap(),
                row["argument"]["index"].as_u64().unwrap()
            ))
        );
        assert_eq!(out.value.get("next").is_some(), offset < 2);
        assert_eq!(
            out.value["argumentCoverage"],
            built.value["argumentCoverage"]
        );
    }
    let calls = ask(&dir.0, "witnesses", Some("args.ts#run"), |o| {
        o.edges = vec!["calls".into()];
        o.to = Some("args.ts#handler".into());
    });
    assert_eq!(calls.value["total"], 0, "{}", calls.value);
    let sink = ask(&dir.0, "callees", Some("args.ts#sink"), |_| {});
    assert_eq!(sink.value["total"], 0);
    let counts = &built.value["argumentCoverage"]["byLanguage"]["typescript"]["counts"];
    assert_eq!(counts["directReferences"], 3);
    let reused = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(
        reused.value["argumentCoverage"],
        built.value["argumentCoverage"]
    );
    incremental::assert_clean_equivalent(&dir.0);
}

#[test]
fn argument_barriers_and_unsupported_forms_remain_queryable() {
    let source = "function handler() {}\nfunction sink(...values: unknown[]) {}\nexport function run(handler: unknown){ sink(handler); sink(...[handler]); sink(1, 'handler', () => handler, handler()); }\n";
    let dir = git_app(&[("barriers.ts", source)]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let linked = ask(&dir.0, "witnesses", Some("barriers.ts#run"), |o| {
        o.edges = vec!["passes".into()];
    });
    assert_eq!(linked.value["total"], 0, "{}", linked.value);
    let mut ids = BTreeSet::new();
    for offset in 0..2 {
        let page = ask(&dir.0, "unresolved", Some("barriers.ts#run"), |o| {
            o.edges = vec!["passes".into()];
            o.limit = Some(1);
            o.offset = Some(offset);
        });
        assert_eq!(page.value["total"], 2, "{}", page.value);
        let row = &page.value["results"][0];
        assert!(ids.insert(row["id"].as_str().unwrap().to_owned()));
        if row["reason"] == "binding-unrepresented" {
            assert_eq!(slice(source, &row["sourceRange"]), "handler");
            assert!(slice(source, &row["bindingSourceRange"]).contains("handler"));
            assert_eq!(row["targetName"], "handler");
        } else {
            assert_eq!(row["reason"], "unsupported-argument-form");
            assert_eq!(row["argument"]["form"], "unsupported");
            assert_eq!(slice(source, &row["sourceRange"]), "...[handler]");
            assert!(
                row["construct"]
                    .as_str()
                    .is_some_and(|value| !value.is_empty())
            );
            assert!(row.get("targetName").is_none());
            assert!(row.get("selectionByteRange").is_none());
        }
        assert!(row.get("candidates").is_none());
    }
    let counts = &built.value["argumentCoverage"]["byLanguage"]["typescript"]["counts"];
    assert_eq!(counts["slots"], 6);
    assert_eq!(counts["directReferences"], 1);
    assert_eq!(counts["unsupportedSlots"], 1);
    assert_eq!(counts["nonReferenceValues"], 4);
}

#[test]
fn argument_coverage_distinguishes_unsupported_and_observed_empty() {
    let dir = git_app(&[
        ("empty.py", "def quiet():\n    pass\n"),
        ("labels.asm", "entry:\n ret\n"),
    ]);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    let coverage = &built.value["argumentCoverage"];
    assert_eq!(coverage["byLanguage"]["python"]["counts"]["slots"], 0);
    assert_eq!(
        coverage["byLanguage"]["asm"],
        json!({"extraction":"unsupported"})
    );
    for operation in ["witnesses", "unresolved"] {
        let out = ask(&dir.0, operation, None, |o| o.edges = vec!["passes".into()]);
        assert_eq!(out.value["total"], 0);
        assert_eq!(&out.value["argumentCoverage"], coverage);
    }
}

#[test]
fn argument_candidates_preserve_noncall_confidence_and_source_encoding() {
    let source = "import {handler} from './barrel';\r\nconst text = 'λ'; export function run(){ sink(handler, handler); }\r\n";
    let dir = git_app(&[
        ("a.ts", "export function handler() {}\n"),
        ("b.ts", "export function handler() {}\n"),
        ("barrel.ts", "export * from './a'; export * from './b';\n"),
        ("encoded.ts", ""),
    ]);
    let encoded = [0xff, 0xfe]
        .into_iter()
        .chain(source.encode_utf16().flat_map(u16::to_le_bytes))
        .collect::<Vec<_>>();
    std::fs::write(dir.0.join("app/encoded.ts"), encoded).unwrap();
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    let out = ask(&dir.0, "witnesses", Some("encoded.ts#run"), |o| {
        o.edges = vec!["passes".into()];
        o.confidence = Some("high".into());
    });
    assert_eq!(out.value["total"], 4, "{}", out.value);
    let mut ids = BTreeSet::new();
    for row in out.value["results"].as_array().unwrap() {
        assert_eq!(row["confidence"], "medium");
        assert_eq!(
            row["sourceDigest"],
            octocode_engine::digest::sha256(source.as_bytes())
        );
        assert_eq!(slice(source, &row["sourceRange"]), "handler");
        assert!(ids.insert(row["id"].as_str().unwrap()));
    }
}

#[test]
fn argument_observations_survive_every_grammar_family_and_macro_boundaries() {
    let fixtures = [
        (
            "a.ts",
            "function handler(){} function main(){ consume(handler, 7, nested(), () => handler); }",
        ),
        (
            "a.py",
            "def handler(): pass\ndef main():\n consume(handler, 7, nested(), lambda: handler)\n",
        ),
        (
            "a.cpp",
            "void handler() {} void main() { consume(handler, 7, nested()); }",
        ),
        (
            "a.go",
            "package p\nfunc handler() {}\nfunc main(){ consume(handler, 7, nested(), func(){}) }",
        ),
        (
            "A.java",
            "class Box { void handler() {} void main() { consume(handler, 7, nested(), () -> handler()); } }",
        ),
        (
            "a.rs",
            "fn handler() {} fn main() { consume(handler, 7, nested(), || handler()); consume!(handler); }",
        ),
        (
            "A.cs",
            "class Box { void handler() {} void Main() { consume(handler, 7, nested(), () => handler()); } }",
        ),
        (
            "A.scala",
            "object Box { def handler() = (); def main() = consume(handler, 7, nested(), () => handler()) }",
        ),
    ];
    let dir = git_app(&fixtures);
    let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    for (file, source) in fixtures {
        let mut references = Vec::new();
        for operation in ["witnesses", "unresolved"] {
            let out = ask(&dir.0, operation, Some(file), |o| {
                o.edges = vec!["passes".into()];
            });
            assert!(out.value["results"].is_array(), "{file}: {}", out.value);
            references.extend(
                out.value["results"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|row| row["targetName"] == "handler")
                    .cloned(),
            );
        }
        assert_eq!(references.len(), 1, "{file}: {references:?}");
        assert_eq!(
            slice(source, &references[0]["selectionByteRange"]),
            "handler"
        );
        assert_eq!(slice(source, &references[0]["sourceRange"]), "handler");
    }
    let gaps = ask(&dir.0, "unresolved", Some("a.rs"), |o| {
        o.edges = vec!["passes".into()];
    });
    let group = gaps.value["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["reason"] == "unsupported-argument-group")
        .expect("macro argument group");
    assert_eq!(group["argument"]["form"], "unsupportedGroup");
    assert!(group["argument"].get("index").is_none());
    assert!(group.get("targetName").is_none());
    assert!(group.get("selectionByteRange").is_none());
    assert!(
        group["construct"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    );
    incremental::assert_clean_equivalent(&dir.0);
}

#[test]
fn wide_argument_lists_keep_every_slot_across_pages() {
    for separator in [",\n", ", "] {
        let source = format!(
            "fn consume() {{}} fn handler() {{}} fn main() {{ consume({}); }}",
            vec!["handler"; 1203].join(separator)
        );
        let dir = git_app(&[("wide.rs", &source)]);
        let built = ingest_root(&dir.0, &dir.0.join("app"), None, false);
        assert_eq!(built.class, ExitClass::Success, "{}", built.value);
        assert_eq!(
            built.value["argumentCoverage"]["byLanguage"]["rust"]["counts"]["directReferences"],
            1203
        );
        assert_eq!(built.value["counts"]["nodesByKind"]["symbol"], 3);
        assert_eq!(built.value["calls"]["sites"], 1);
        assert_eq!(built.value["calls"]["linked"], 1);
        let calls = ask(&dir.0, "witnesses", Some("wide.rs"), |o| {
            o.edges = vec!["calls".into()];
        });
        assert_eq!(calls.value["total"], 1);
        assert_eq!(calls.value["results"][0]["to"], "wide.rs#consume");
        let mut ids = BTreeSet::new();
        let mut slots = BTreeSet::new();
        for offset in (0..1203).step_by(71) {
            let out = ask(&dir.0, "witnesses", Some("wide.rs"), |o| {
                o.edges = vec!["passes".into()];
                o.limit = Some(71);
                o.offset = Some(offset);
            });
            assert_eq!(out.value["total"], 1203, "{}", out.value);
            for row in out.value["results"].as_array().unwrap() {
                assert_eq!(row["from"], "wide.rs#main");
                assert_eq!(row["to"], "wide.rs#handler");
                assert!(ids.insert(row["id"].as_str().unwrap().to_owned()));
                assert!(slots.insert(row["argument"]["index"].as_u64().unwrap()));
            }
            assert_eq!(out.value.get("next").is_some(), offset + 71 < 1203);
        }
        assert_eq!(ids.len(), 1203);
        assert_eq!(slots, (0..1203).collect());
    }
}
