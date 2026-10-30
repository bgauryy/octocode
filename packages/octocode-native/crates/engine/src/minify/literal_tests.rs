use super::apply::apply_content_view_minification_inner as compact;

#[test]
fn javascript_compaction_preserves_multiline_literal_bytes() {
    for extension in crate::text::JS_TS_EXTENSIONS {
        for literal in [
            "`first\n  // keep\n  /* keep */`",
            "`first  \r\n\r\n\r\n  שלום😀\t\r\nlast`",
            "`first\\` escaped\n  last`",
            "`first ${value}\n  last`",
            "\"first\\\n  second\"",
        ] {
            let source = format!("// remove\nconst value = 1;\nconst text = {literal};\n");
            let output = compact(&source, &format!("fixture.{extension}"));
            assert!(output.contains(literal), "{extension}: {output:?}");
            assert!(!output.contains("// remove"), "{extension}: {output:?}");
        }
    }
}

#[test]
fn incomplete_literals_keep_their_payload_to_the_end_of_the_selection() {
    for (extension, source, payload) in [
        (
            "js",
            "const text = `first\n  second  \n\n",
            "`first\n  second  \n\n",
        ),
        (
            "py",
            "text = \"first\n  second  \n\n",
            "\"first\n  second  \n\n",
        ),
    ] {
        let output = compact(source, &format!("fixture.{extension}"));
        assert!(output.contains(payload), "{extension}: {output:?}");
    }
}

#[test]
fn yaml_compaction_preserves_scalars_and_partial_documents_verbatim() {
    let sources = [
        "# header\nname: \"# literal\"\nscript: |\n  # preserve in scalar\n  echo hello\nitems:\n  - one\n  - two\n",
        "literal: |+\n  first  \n\n  last\n\nfolded: >-\n  first\n\n  second\n",
        "nested:\n  value: |2-\n    # payload\n    שלום😀  \nnext: true\n",
        "list:\n  - >+2\n    first\n\n    last\n",
        "value: \"first\n  second\"\nother: 'it''s # text'\n",
        "  # selected inside a scalar\r\n\r\n  echo retained  \r\n",
        "value: |\n\n",
    ];
    for extension in ["yaml", "yml", "YAML", "YML"] {
        for source in sources {
            assert_eq!(
                compact(source, &format!("fixture.{extension}")),
                source,
                "{extension}"
            );
        }
    }
}

#[test]
fn nested_template_expressions_preserve_literal_payloads() {
    for literal in [
        "`outer ${`inner\n  // literal\n  tail`} end`",
        "`outer ${\"`\"} end\n  // literal`",
        "`outer ${{a: `nested ${1} \n  // keep`}.a} tail`",
        "`outer ${/[}]/.test('}') ? `yes` : `no`} tail\n  // keep`",
        "`outer ${/* } ` */ `inner`} end\n  // keep`",
    ] {
        let source = format!("// remove\nglobalThis.result = {literal};\n");
        let output = compact(&source, "nested.js");
        assert!(output.contains(literal), "{output:?}");
        assert!(!output.contains("// remove"), "{output:?}");
    }
}
