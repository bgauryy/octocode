use super::*;

#[test]
fn health_matches_reachability_across_word_boundaries() {
    let n = 137usize;
    let edges = (0..n)
        .map(|from| {
            (0..n)
                .filter(|&to| {
                    // Disconnected nodes, diamonds, self loops and weighted SCCs.
                    from < 120
                        && (to == from + 1 || to == from + 3 || (from % 11 == 0 && to + 2 == from))
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let sources = edges
        .iter()
        .enumerate()
        .map(|(from, targets)| {
            let imports = targets
                .iter()
                .map(|to| format!("import './f{to}';\n"))
                .collect::<String>();
            (
                format!("f{from}.ts"),
                format!("{imports}export const value = {from};\n"),
            )
        })
        .collect::<Vec<_>>();
    let borrowed = sources
        .iter()
        .map(|(path, source)| (path.as_str(), source.as_str()))
        .collect::<Vec<_>>();
    let dir = git_app(&borrowed);
    assert_eq!(ingest_fixture(&dir.0, None).class, ExitClass::Success);
    let expected = (0..n)
        .map(|start| {
            let mut seen = std::collections::BTreeSet::new();
            let mut pending = vec![start];
            while let Some(id) = pending.pop() {
                if seen.insert(id) {
                    pending.extend(&edges[id]);
                }
            }
            seen.len() as u64
        })
        .sum::<u64>();
    let out = ask(&dir.0, "issues", None, |o| {
        o.detectors = vec!["cycle".into()];
    });
    assert_eq!(out.value["summary"]["health"]["files"], n);
    assert_eq!(
        out.value["summary"]["health"]["ccd"], expected,
        "{}",
        out.value
    );
}
