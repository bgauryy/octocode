use super::*;

fn unnumber(content: &str) -> String {
    content
        .split_inclusive('\n')
        .map(|line| line.split_once('\t').expect("gutter").1)
        .collect()
}

#[test]
fn compact_yaml_preserves_values_and_exact_source_numbers() {
    let t = Fixture::new();
    let source = "# header\nname: \"# literal\"\nscript: |\n  # preserve in scalar\n  echo hello\n\n  tail  \nfolded: >-\n  first\n\n  second\n";
    for extension in ["yaml", "yml", "YAML", "YML"] {
        let path = t.0.join(format!("source.{extension}"));
        fs::write(&path, source).expect("source");
        for (fields, expected, numbers) in [
            (
                serde_json::json!({"view":"compact"}),
                source.to_owned(),
                (1..=11).collect::<Vec<_>>(),
            ),
            (
                serde_json::json!({"view":"compact","ranges":[{"startLine":4,"endLine":7}]}),
                "  # preserve in scalar\n  echo hello\n\n  tail  \n".to_owned(),
                vec![4, 5, 6, 7],
            ),
        ] {
            let result = fetch(&qj(&path, fields), &Paths(t.0.clone()));
            assert!(result.error.is_none(), "{result:?}");
            let content = result.content.as_deref().expect("content");
            assert_eq!(unnumber(content), expected, "{extension}");
            let labels: Vec<usize> = content
                .lines()
                .map(|line| {
                    line.split_once('\t')
                        .expect("gutter")
                        .0
                        .parse()
                        .expect("number")
                })
                .collect();
            assert_eq!(labels, numbers, "{extension}");
        }
    }
}

#[test]
fn compact_javascript_preserves_multiline_literal_payload_and_lines() {
    let t = Fixture::new();
    let path = t.0.join("literal.js");
    let source = "// removed\nconst text = `first  \n\n  // keep\n  /* keep */\nlast`;\n";
    fs::write(&path, source).expect("source");
    let result = fetch(
        &qj(&path, serde_json::json!({"view":"compact"})),
        &Paths(t.0.clone()),
    );
    let content = result.content.as_deref().expect("content");
    assert!(
        unnumber(content).contains("`first  \n\n  // keep\n  /* keep */\nlast`"),
        "{content:?}"
    );
    assert!(content.contains("3\t\n4\t  // keep\n"), "{content:?}");
}

#[test]
fn compact_ranges_keep_the_literal_context_outside_the_selection() {
    let t = Fixture::new();
    for (extension, source, expected) in [
        (
            "js",
            "const value = `first\n  // payload  \n\n  last`;\n// remove\nconst n = 1;\n",
            "2\t  // payload  \n3\t\n4\t  last`;\n--\n6\tconst n = 1;",
        ),
        (
            "py",
            "value = \"\"\"first\n  # payload  \n\n  last\"\"\"\n# remove\nn = 1\n",
            "2\t  # payload  \n3\t\n4\t  last\"\"\"\n--\n6\tn = 1",
        ),
    ] {
        let path = t.0.join(format!("ranged.{extension}"));
        fs::write(&path, source).expect("source");
        let result = fetch(
            &qj(
                &path,
                serde_json::json!({"view":"compact","ranges":[{"startLine":2,"endLine":4},{"startLine":6,"endLine":6}]}),
            ),
            &Paths(t.0.clone()),
        );
        assert_eq!(
            result
                .content
                .as_deref()
                .expect("content")
                .trim_end_matches('\n'),
            expected,
            "{extension}"
        );
    }
}
