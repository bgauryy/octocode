use super::*;

const PACKAGE: &str = r#"{
  "name": "vitest",
  "keywords": [
    "vite",
    "test"
  ],
  "peerDependencies": {
    "vite": "^7"
  },
  "dependencies": {
    "@vitest/mocker": "workspace:*"
  },
  "devDependencies": {
    "vite": "^7"
  },
  "exports": {
    "./node": { "import": "./dist/node.js" }
  }
}
"#;

/// The same entry reads differently under each section.
#[test]
fn hits_name_their_section() {
    assert_eq!(
        line_key_paths(PACKAGE, &[4, 8, 11, 14]),
        vec![
            (4, "keywords[0]".to_owned()),
            (8, "peerDependencies.vite".to_owned()),
            (11, "dependencies.@vitest/mocker".to_owned()),
            (14, "devDependencies.vite".to_owned()),
        ]
    );
}

/// A line that opens or closes a container names it; the root is left out.
#[test]
fn container_lines_name_the_container() {
    assert_eq!(
        line_key_paths(PACKAGE, &[1, 7, 9, 19]),
        vec![
            (7, "peerDependencies".to_owned()),
            (9, "peerDependencies".to_owned()),
        ]
    );
}

/// A key that a dot or bracket would misread is quoted.
#[test]
fn keys_with_dots_are_quoted() {
    assert_eq!(
        line_key_paths(PACKAGE, &[17]),
        vec![(17, r#"exports["./node"]"#.to_owned())]
    );
}

/// Comments, trailing commas and unquoted keys (JSONC/JSON5) are read
/// through; array elements count past them.
#[test]
fn tolerates_jsonc() {
    let text = "{\n  // a comment, with \"quotes\" {\n  compilerOptions: {\n    /* block\n } */ \"strict\": true,\n  },\n  \"files\": [\"a\", /* x */ \"b\",\n    \"c\",\n  ],\n}\n";
    assert_eq!(
        line_key_paths(text, &[2, 3, 5, 8]),
        vec![
            (3, "compilerOptions".to_owned()),
            (5, "compilerOptions.strict".to_owned()),
            (8, "files[2]".to_owned()),
        ]
    );
}

/// Broken nesting degrades to the containers still open, never fails.
#[test]
fn broken_json_keeps_scanning() {
    let text = "{\n  \"a\": {\n    \"b\": [1, 2\n  },\n  \"c\": \"unterminated\n  \"d\": 1\n}\n";
    let found = line_key_paths(text, &[3, 6]);
    assert_eq!(found[0], (3, "a.b".to_owned()));
    assert_eq!(found.len(), 2, "{found:?}");
}
