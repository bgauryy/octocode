//! Parent texts a text check could misread, each decided as the parse decides.
use super::*;

/// A `mod` or a `cfg` a comment or a literal spells is no mount, whatever a
/// text check takes it for; the parse decides.
#[test]
fn comments_and_literals_mount_nothing() {
    let all = ["a", "b"];
    for parent in [
        "// #[cfg(test)]\n// mod a;\nmod b;\n",
        "/* #[cfg(test)]\nmod a; */\n",
        "/* /* #[cfg(test)] */ mod a; */\n",
        "const S: &str = \"#[cfg(test)]\\nmod a;\";\n",
        "const S: &str = r#\"#[cfg(test)]\nmod a;\"#;\n",
        "const S: &[u8] = b\"#[cfg(test)] mod a;\";\n",
        "/// #[cfg(test)]\n/// mod a;\nfn f() {}\n",
        "const C: char = '\"';\nconst S: &str = \"#[cfg(test)] mod a;\";\n",
    ] {
        assert_eq!(mounted(parent, &all), Vec::<String>::new(), "{parent}");
    }
}

/// A literal that reads like it opens a string hides no gated mount after it.
#[test]
fn a_literal_before_a_gated_mount_does_not_hide_it() {
    let all = ["a"];
    for parent in [
        "const Q: char = '\"';\n#[cfg(test)]\nmod a;\n",
        "const Q: char = '\\\\';\n#[cfg(test)]\nmod a;\n",
        "const Q: [char; 2] = ['é','\"'];\n#[cfg(test)]\nmod a;\nconst E: &str = \"\";\n",
        "const Q: &str = \"\\\"\";\n#[cfg(test)]\nmod a;\n",
        "fn f<'a>(x: &'a str) {}\n#[cfg(test)]\nmod a;\n",
    ] {
        assert_eq!(mounted(parent, &all), ["a"], "{parent}");
    }
}

/// A `mod` in a test-only body is mounted under test, whatever the delimiters
/// around it, and whatever `cfg` the file spelled before it.
#[test]
fn a_block_local_mount_in_a_test_item_is_mounted_under_test() {
    let all = ["a"];
    for parent in [
        "#[cfg(test)]\nfn f() {\n    mod a;\n}\n",
        "#[cfg(test)]\nconst _: () = {\n    mod a;\n};\n",
        "#[cfg(test)]\nimpl T {\n    fn f() {\n        mod a;\n    }\n}\n",
        "// c\n#[cfg(test)]\nfn f() {\n    #[path = \"a.rs\"]\n    mod a;\n}\n",
        "#[cfg(test)] use std::fmt;\n#[cfg(test)]\nfn f() {\n    mod a;\n}\n",
        "#![cfg_attr(test, allow(unused))]\n#[cfg(test)]\nfn f() {\n    mod a;\n}\n",
        "fn f() {\n    #![cfg(test)]\n    mod a;\n}\n",
        "fn f() {\n    mod a;\n    #![cfg(test)]\n}\n",
    ] {
        assert_eq!(mounted(parent, &all), ["a"], "{parent}");
    }
}

/// `;`, `{` and `}` inside an attribute end no statement: the gate reaches the
/// `mod` the attribute run precedes.
#[test]
fn delimiters_inside_an_attribute_do_not_end_the_gate() {
    let all = ["a"];
    for parent in [
        "// c\n#[cfg(test)]\n#[foo(x; y)]\nmod a;\n",
        "// c\n#[cfg(test)]\n#[foo({ x })]\nmod a;\n",
        "// c\n#[foo({ x })]\n#[cfg(test)]\nmod a;\n",
        "// c\n#[cfg(test)]\n#[bar = [u8; 4]]\nmod a;\n",
        "// c\n#[cfg(all(test, feature = \"x;y\"))]\nmod a;\n",
    ] {
        assert_eq!(mounted(parent, &all), ["a"], "{parent}");
    }
}

/// A span holds whole lines: a `mod` on a line a test item touches is on it.
#[test]
fn a_mount_sharing_a_line_with_a_test_item_is_decided_by_the_line() {
    let all = ["a"];
    for parent in [
        "#[cfg(test)] fn f() {} mod a;\n",
        "mod a; #[cfg(test)] fn f() {}\n",
        "#[cfg(test)] fn f() {\n} mod a;\n",
        "// c\n#[cfg(test)] fn f() {} mod a;\n",
        "// c\nmod a; #[cfg(test)] fn f() {}\n",
        "// c\n#[cfg(test)] fn f() {\n} mod a;\n",
    ] {
        assert_eq!(mounted(parent, &all), ["a"], "{parent}");
    }
    assert!(mounted("mod a; #[cfg(test)]\nfn f() {}\n", &all).is_empty());
}

/// Whatever comes before a mount, the decision is the parse's: literals a
/// scanner could misread, broken text, and every `cfg` spelling around one.
#[test]
fn every_text_around_a_mount_is_decided_as_the_parse_does() {
    let before = [
        "",
        "// cfg test\n",
        "/* cfg(test) { */\n",
        "/* open /* nested */ ; */\n",
        "#![cfg_attr(test, allow(unused))]\n",
        "#![cfg(not(test))]\n",
        "#![ cfg (test)]\n",
        "#! [cfg(test)]\n",
        "# ! [cfg(test)]\n",
        "#![/*x*/cfg(test)]\n",
        "#[cfg(test)] use std::fmt;\n",
        "#[cfg(unix)]\nmod z;\n",
        "#[\ncfg(\ntest)]\n",
        "const Q: char = 'é';\n",
        "const Q: char = '😀';\n",
        "const Q: char = '\"';\n",
        "const Q: char = '\\'';\n",
        "const Q: char = '\\u{1F600}';\n",
        "const Q: u8 = b'{';\n",
        "fn f() { 'a: loop { break 'a; } }\n",
        "const S: &str = \"a\\\n;{\";\n",
        "const S: &[u8] = b\"a\\\";{\";\n",
        "const S: &[u8] = br##\"a\"#;{\"##;\n",
        "const S: &str = r#\"#[cfg(test)] mod b;\"#;\n",
        "m!('\"');\n",
        "\"open\n",
        "/* open\n",
        "fn f( {\n",
        "}\n",
    ];
    let mounts = [
        "mod a;\n",
        "#[cfg(test)]\nmod a;\n",
        "#[cfg( test )]\npub(crate) mod a;\nmod b;\n",
        "#[cfg(\n  test\n)]\nmod\n  a;\n",
        "#[cfg(all(unix, test))]\nmod r#a;\n",
        "#[cfg(any(test, unix))]\nmod a;\n",
        "#[cfg(test)]\n/// doc; {\nmod a;\n",
        "#[cfg(test)]\n/** doc; { */\nmod a;\n",
        "#[cfg(test)]\nmod b;\nmod a;\n",
        "#[cfg(test)]\nfn f() {\n    mod a;\n}\nmod b;\n",
        "#[cfg(test)]\nmod inner {\n    mod a;\n}\n",
        "#[cfg(test)] fn f() {} mod a;\n",
        "cfg_if::cfg_if! { if #[cfg(test)] { mod a; } }\n",
    ];
    let root = fixture("", &["a", "b", "z"]);
    let whole = root.write("src/whole.rs", "");
    for before in before {
        for mount in mounts {
            root.write("src/lib.rs", format!("{before}{mount}"));
            assert_same_mounts(&root.0);
            std::fs::write(&whole, format!("{before}{mount}")).expect("whole");
            assert_same_whole_file(&root.0, &whole);
        }
    }
}
