//! Calls inside nested functions belong to the nested function's node.
use super::*;

#[test]
fn nested_function_calls_link_to_the_innermost_function() {
    let dir = git_app(&[(
        "m.mjs",
        "export function outer(src) {\n  const blank = (l) => !l.trim();\n  function helper(x) {\n    return x + 1;\n  }\n  function blocks(ls) {\n    if (blank(ls[0])) return helper(1);\n    return 0;\n  }\n  return blocks(src);\n}\n",
    )]);
    let root = dir.0.as_path();
    let built = ingest_fixture(root, None);
    assert_eq!(built.class, ExitClass::Success, "{}", built.value);
    for (operation, target, expected) in [
        (
            "callees",
            "m.mjs#outer.blocks",
            &["m.mjs#outer.blank", "m.mjs#outer.helper"][..],
        ),
        ("callers", "m.mjs#outer.blank", &["m.mjs#outer.blocks"][..]),
        ("callers", "m.mjs#outer.helper", &["m.mjs#outer.blocks"][..]),
        ("callees", "m.mjs#outer", &["m.mjs#outer.blocks"][..]),
    ] {
        let answer = ask(root, operation, Some(target), |_| {});
        let mut actual = ids(&answer);
        actual.sort();
        assert_eq!(actual, expected, "{operation} {target}: {}", answer.value);
    }
}

/// The brainstorming report case: `blocks()` calls `blank()`, both nested in
/// one exported function.
#[test]
fn a_nested_helper_called_by_a_nested_sibling_is_linked() {
    let dir = git_app(&[(
        "markdown.mjs",
        "export function renderMarkdown(text) {\n  const lines = text.split('\\n');\n  const blank = (line) => line.trim() === '';\n  const blocks = () => {\n    const out = [];\n    for (const line of lines) {\n      if (blank(line)) continue;\n      out.push(line);\n    }\n    return out;\n  };\n  return blocks().join('\\n');\n}\n",
    )]);
    let root = dir.0.as_path();
    assert_eq!(ingest_fixture(root, None).class, ExitClass::Success);
    let answer = ask(
        root,
        "callers",
        Some("markdown.mjs#renderMarkdown.blank"),
        |_| {},
    );
    assert_eq!(
        ids(&answer),
        vec!["markdown.mjs#renderMarkdown.blocks".to_owned()],
        "{}",
        answer.value
    );
}
