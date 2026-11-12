use super::*;

#[test]
fn graph_ranking_lists_every_qualifying_critical_file() {
    let hub_count = 11;
    let mut files = vec![("package.json".to_owned(), r#"{"name":"app","main":"index.ts"}"#.to_owned())];
    let mut entry = String::new();
    let mut expected = std::collections::BTreeSet::new();
    for hub in 0..hub_count {
        let name = format!("hub-{hub}.ts");
        expected.insert(format!("app/{name}"));
        files.push((name, "export const value = 1;\n".into()));
        for caller in 0..5 {
            let name = format!("caller-{hub}-{caller}.ts");
            entry.push_str(&format!("import './{name}';\n"));
            files.push((name, format!("import {{ value }} from './hub-{hub}';\nvoid value;\n")));
        }
    }
    files.push(("index.ts".into(), entry));
    let borrowed = files.iter().map(|(p, s)| (p.as_str(), s.as_str())).collect::<Vec<_>>();
    let fixture = git_app(&borrowed);
    assert_eq!(ingest_fixture(&fixture.0, None).class, ExitClass::Success);
    let out = ask(&fixture.0, "issues", None, |o| {
        o.detectors = vec!["critical-file".into()];
        o.limit = Some(files.len());
    });
    let found = out.value["rows"].as_array().expect("rows").iter()
        .map(|row| row["subject"].as_str().expect("subject").to_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(found, expected, "{}", out.value);
}

#[test]
fn graph_ranking_equal_dependencies_receive_equal_issue_scores() {
    let fixture = git_app(&[
        ("package.json", r#"{"name":"app","main":"index.ts"}"#),
        ("index.ts", "import { used as a } from './a';\nimport { used as b } from './b';\na(); b();\n"),
        ("a.ts", "export function used() { return 1; }\nexport function spare() { return 2; }\n"),
        ("b.ts", "export function used() { return 1; }\nexport function spare() { return 2; }\n"),
    ]);
    assert_eq!(ingest_fixture(&fixture.0, None).class, ExitClass::Success);
    let out = ask(&fixture.0, "issues", None, |o| {
        o.detectors = vec!["unused-export".into()];
        o.min_tier = Some(60);
    });
    let rows = out.value["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 2, "{}", out.value);
    assert_eq!(rows[0]["severity"], rows[1]["severity"]);
    assert_eq!(rows[0]["confidence"], rows[1]["confidence"]);
    assert_eq!(rows[0]["impact"], rows[1]["impact"], "{}", out.value);
    assert_eq!(rows[0]["score"], rows[1]["score"], "{}", out.value);
}
