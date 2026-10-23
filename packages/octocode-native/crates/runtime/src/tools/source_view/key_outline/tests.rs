use super::*;

#[test]
fn key_outline_merges_touching_and_overlapping_unparsed_spans() {
    let outline = key_outline(
        "title = 'ok'\n[broken\n[also_broken\n[valid]\na = 1\n",
        "bad.toml",
    )
    .expect("outline");
    assert_eq!(outline.unparsed.as_deref(), Some("2-3"));
    assert!(outline.text.contains("[valid]"), "{}", outline.text);
}

#[test]
fn json_outlines_member_keys_with_container_spans() {
    let json = "{\n  \"name\": \"x\",\n  \"scripts\": {\n    \"build\": \"tsc\",\n    \"nested\": {\n      \"deep\": {\n        \"deeper\": [1]\n      }\n    }\n  },\n  \"list\": [\n    {\"a\": 1}\n  ]\n}\n";
    let outline = key_outline(json, "package.json").expect("outline").text;
    assert_eq!(
        outline,
        "2\t  \"name\": \"x\",\n3-10\t  \"scripts\": {\n5-9\t    \"nested\": {\n6-8\t      \"deep\": {\n11-13\t  \"list\": [\n"
    );
}

#[test]
fn json_strings_and_comments_never_open_containers() {
    let json = "// {\n{\n  /* [ */ \"a\": \"{[\",\n  \"b\": {\n    \"c\": 1\n  }\n}\n";
    assert_eq!(
        key_outline(json, "a.jsonc").expect("outline").text,
        "3\t  /* [ */ \"a\": \"{[\",\n4-6\t  \"b\": {\n"
    );
}

/// Each object of an array spans from its opening line and shows the
/// member that tells it apart: `name` is the key every object holds as
/// a different string (`kind` repeats, `beta` is no string).
#[test]
fn json_array_objects_are_named_by_their_distinct_member() {
    let json = "{\n  \"tools\": [\n    {\n      \"beta\": true,\n      \"kind\": \"local\",\n      \"name\": \"fetch\",\n      \"schema\": {\n        \"a\": 1\n      }\n    },\n    {\n      \"beta\": false,\n      \"kind\": \"local\",\n      \"name\": \"search\",\n      \"schema\": {\n        \"a\": 1\n      }\n    }\n  ]\n}\n";
    assert_eq!(
        key_outline(json, "contract.json").expect("outline").text,
        "2-19\t  \"tools\": [\n3-10\t    {\n6\t      \"name\": \"fetch\",\n7-9\t      \"schema\": {\n11-18\t    {\n14\t      \"name\": \"search\",\n15-17\t      \"schema\": {\n"
    );
}

/// Of two distinct string members, the shorter values name the objects.
#[test]
fn json_array_names_prefer_the_shortest_distinct_values() {
    let json = "[\n  {\n    \"about\": \"the first long description\",\n    \"id\": \"a\",\n    \"n\": [1]\n  },\n  {\n    \"about\": \"the second long description\",\n    \"id\": \"b\",\n    \"n\": [2]\n  }\n]\n";
    assert_eq!(
        key_outline(json, "a.json").expect("outline").text,
        "2-6\t  {\n4\t    \"id\": \"a\",\n7-11\t  {\n9\t    \"id\": \"b\",\n"
    );
}

#[test]
fn yaml_outlines_keys_by_indentation() {
    let yaml = "name: ci\non:\n  push:\n    branches: [main]\njobs:\n  build:\n    steps:\n      - name: test\n        run: |\n          cargo test\n";
    assert_eq!(
        key_outline(yaml, "ci.yml").expect("outline").text,
        "1\tname: ci\n2-4\ton:\n3-4\t  push:\n5-10\tjobs:\n6-10\t  build:\n7-10\t    steps:\n"
    );
}

/// Text inside a block scalar or a multi-line quoted scalar is never a
/// key, however it is shaped.
#[test]
fn yaml_block_and_quoted_scalars_are_text() {
    let yaml = "name: demo\nscript: |\n  fake: value\n  other:\n    nested: 1\n\n  more: text\nfolded: >-\n  a: b\nquoted: \"first\n  inner: x\n  end\"\nlist:\n  - |\n    item: text\n  - plain\njobs:\n  build:\n    run: echo\n";
    assert_eq!(
        key_outline(yaml, "k.yaml").expect("outline").text,
        "1\tname: demo\n2-7\tscript: |\n8-9\tfolded: >-\n10-12\tquoted: \"first\n13-16\tlist:\n17-19\tjobs:\n18-19\t  build:\n"
    );
}

#[test]
fn toml_outlines_tables_and_top_level_keys() {
    let toml = "name = \"x\"\n\n[package]\nversion = \"1\"\n\n[[bin]]\nname = \"b\"\n";
    assert_eq!(
        key_outline(toml, "Cargo.toml").expect("outline").text,
        "1\tname = \"x\"\n3-4\t[package]\n6-7\t[[bin]]\n"
    );
}

/// The parser places tables: a `[...]` line inside a multi-line string
/// or array is a value, and a multi-line value spans its lines.
#[test]
fn toml_strings_and_arrays_never_open_tables() {
    let toml = "title = \"x\"\nmulti = \"\"\"\n[not.a.table]\nkey = 1\n\"\"\"\nlist = [\n  [1, 2],\n]\n[server]\nport = 80\n# trailing comment\n\n[[servers.list]]\nname = \"a\"\n[[servers.list]]\nname = \"b\"\n";
    assert_eq!(
        key_outline(toml, "k.toml").expect("outline").text,
        "1\ttitle = \"x\"\n2-5\tmulti = \"\"\"\n6-8\tlist = [\n9-10\t[server]\n13-14\t[[servers.list]]\n15-16\t[[servers.list]]\n"
    );
}

/// A malformed header is skipped, not read as a table running to the
/// next one that parses: its keys read as top-level ones, the tables
/// after it are listed, and the outline names the skipped line.
#[test]
fn toml_resumes_after_a_line_it_cannot_parse() {
    let toml = "title = \"x\"\n[server\nport = 80\n[db]\nurl = 'a'\nlit = '''\n[fake.table]\n'''\n[[arr]]\nn = 1\n[[arr]]\nn = 2\n";
    let outline = key_outline(toml, "bad.toml").expect("outline");
    assert_eq!(
        outline.text,
        "1\ttitle = \"x\"\n3\tport = 80\n4-8\t[db]\n9-10\t[[arr]]\n11-12\t[[arr]]\n"
    );
    assert_eq!(outline.unparsed.as_deref(), Some("2"));
    let clean = key_outline("[a]\nb = 1\n", "ok.toml").expect("outline");
    assert_eq!(clean.unparsed, None);
}

#[test]
fn other_files_have_no_key_outline() {
    assert!(key_outline("a: 1\n", "notes.txt").is_none());
    assert!(key_outline("[1, 2]\n", "a.json").is_none());
}
