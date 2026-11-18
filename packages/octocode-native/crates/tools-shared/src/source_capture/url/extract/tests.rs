use super::dom::{Data, Dom, Node};
use super::*;
use html5ever::{LocalName, Namespace, QualName};

const GUIDE: &str = r#"<!doctype html><html><head><title>Guide</title>
<style>body{color:red}</style><script>var hidden = "script text";</script></head>
<body>
<nav><a href="/">Home</a> <a href="/docs">Docs</a></nav>
<h1 id="top">Key   rotation
guide</h1>
<p>Rotate keys <b>every</b> 90&nbsp;days.<br>Then restart.</p>
<h2 id="rotation">Rotation</h2>
<p>Update the configuration.</p>
<ul><li>Stop the <em>service</em></li><li>Swap the key<ul><li>Nested step</li></ul></li></ul>
<h3 id="verify">Verify</h3>
<table><tr><th>Step</th><th>Command</th></tr><tr><td>Check</td><td><code>svc status</code></td></tr></table>
<pre><code>fn main() {
    rotate();
}
</code></pre>
<h2 id="faq">FAQ</h2>
<p>Nothing else.</p>
<template><p>template text</p></template><noscript>enable js</noscript>
<svg><text>svg text</text></svg>
<footer>Copyright</footer>
</body></html>"#;

fn html(source: &str, fragment: Option<&str>) -> Result<String, Failure> {
    super::html(source, fragment, &|| false)
}

#[test]
fn a_page_renders_blocks_as_lines_in_document_order() {
    let text = html(GUIDE, None).unwrap();
    assert_eq!(
        text,
        "# Key rotation guide\n\
             Rotate keys every 90\u{a0}days.\n\
             Then restart.\n\
             ## Rotation\n\
             Update the configuration.\n\
             - Stop the service\n\
             - Swap the key\n  \
             - Nested step\n\
             ### Verify\n\
             Step | Command\n\
             Check | svc status\n\
             ```\n\
             fn main() {\n    rotate();\n}\n\
             ```\n\
             ## FAQ\n\
             Nothing else.\n"
    );
    for dropped in [
        "script text",
        "color:red",
        "Home",
        "template text",
        "enable js",
        "svg text",
        "Copyright",
        "Guide\n",
    ] {
        assert!(!text.contains(dropped), "{dropped} leaked: {text}");
    }
}

#[test]
fn a_heading_fragment_selects_through_the_next_same_or_higher_heading() {
    assert_eq!(
        html(GUIDE, Some("rotation")).unwrap(),
        "## Rotation\nUpdate the configuration.\n- Stop the service\n- Swap the key\n  - Nested step\n\
             ### Verify\nStep | Command\nCheck | svc status\n```\nfn main() {\n    rotate();\n}\n```\n"
    );
    assert_eq!(
        html(GUIDE, Some("verify")).unwrap(),
        "### Verify\nStep | Command\nCheck | svc status\n```\nfn main() {\n    rotate();\n}\n```\n"
    );
    assert_eq!(html(GUIDE, Some("faq")).unwrap(), "## FAQ\nNothing else.\n");
    let top = html(GUIDE, Some("top")).unwrap();
    assert!(top.starts_with("# Key rotation guide\n") && top.ends_with("Nothing else.\n"));
}

#[test]
fn an_anchor_inside_or_before_a_heading_selects_that_heading_section() {
    let page = r#"<h2><a name="inner"></a>Inner</h2><p>a</p><a id="before"></a><h2>Before</h2><p>b</p><h2>Next</h2><p>c</p>"#;
    assert_eq!(html(page, Some("inner")).unwrap(), "## Inner\na\n");
    assert_eq!(html(page, Some("before")).unwrap(), "## Before\nb\n");
}

#[test]
fn a_non_heading_anchor_selects_its_own_element() {
    let page = r#"<p>intro</p><section id="s"><p>one</p><ul><li>two</li></ul></section><p>after</p>
            <p>Run <code id="cmd">svc restart</code> now.</p>"#;
    assert_eq!(html(page, Some("s")).unwrap(), "one\n- two\n");
    assert_eq!(html(page, Some("cmd")).unwrap(), "Run svc restart now.\n");
}

#[test]
fn a_missing_or_duplicate_anchor_fails_explicitly() {
    assert_eq!(html(GUIDE, Some("absent")), Err(Failure::MissingAnchor));
    let page = r#"<h2 id="x">One</h2><p>a</p><a name="x"></a><h2>Two</h2>"#;
    assert_eq!(html(page, Some("x")), Err(Failure::AmbiguousAnchor(2)));
    assert_eq!(
        html(r#"<p>text</p><nav id="menu">links</nav>"#, Some("menu")),
        Err(Failure::EmptySection)
    );
}

#[test]
fn an_empty_page_says_whether_scripts_build_it() {
    assert_eq!(
        html(
            r#"<html><body><div id="app"></div><script src="app.js"></script></body></html>"#,
            None
        ),
        Err(Failure::Empty { scripted: true })
    );
    assert_eq!(
        html("<p>   </p>", None),
        Err(Failure::Empty { scripted: false })
    );
}

#[test]
fn misnested_markup_nests_as_a_browser_shows_it() {
    // Unclosed paragraphs and list items close implicitly; a stray end
    // tag is ignored; the table cell outside a row gets one.
    let page = "<p>one<p>two<ul><li>a<li>b</ul></span><table><td>cell</td></table>";
    assert_eq!(html(page, None).unwrap(), "one\ntwo\n- a\n- b\ncell\n");
}

#[test]
fn deep_nesting_does_not_recurse() {
    // A million nested elements built directly: the parser's own cost
    // is bounded by `stop`, the walk and lookups must not recurse.
    let depth = 1_000_000;
    let mut nodes = vec![Node {
        parent: None,
        children: vec![1],
        data: Data::Document,
    }];
    for id in 1..=depth {
        let name = if id == 1 { "body" } else { "div" };
        nodes.push(Node {
            parent: Some(id - 1),
            children: vec![id + 1],
            data: Data::Element {
                name: QualName::new(None, Namespace::from(""), LocalName::from(name)),
                attrs: Vec::new(),
                contents: None,
            },
        });
    }
    nodes.push(Node {
        parent: Some(depth),
        children: Vec::new(),
        data: Data::Text("deep".into()),
    });
    let dom = Dom { nodes };
    let walked = Walker::new(&dom, Some(depth)).walk(dom.body());
    assert_eq!(join(&walked.lines), "deep\n");
    assert_eq!(walked.span, Some((0, 1)));
    assert_eq!(dom.heading_at_or_above(depth + 1), None);
    assert_eq!(dom.inline_text(1), "deep");
}

#[test]
fn a_stop_ends_the_parse() {
    let calls = std::cell::Cell::new(0);
    let page = "<p>text</p>".repeat(4 * 1024);
    let stop = || {
        calls.set(calls.get() + 1);
        calls.get() > 2
    };
    assert_eq!(super::html(&page, None, &stop), Err(Failure::Stopped));
    assert_eq!(calls.get(), 3, "checked once per chunk until it stops");
    assert!(super::html(&page, None, &|| false).is_ok());
}

#[test]
fn a_chunk_boundary_inside_a_character_or_tag_parses_alike() {
    let filler = "é".repeat(PARSE_CHUNK_BYTES / 2 - 1);
    let page = format!("<p>{filler}x</p><h2 id=\"a\">Ü</h2><p>end</p>");
    assert!(page.len() > PARSE_CHUNK_BYTES);
    assert_eq!(
        html(&page, Some("a")).unwrap(),
        "## Ü\nend\n",
        "a split multibyte character or tag still parses"
    );
    assert!(
        html(&page, None)
            .unwrap()
            .starts_with(&format!("{filler}x\n"))
    );
}

#[test]
fn plain_text_normalizes_line_endings_and_ends_with_a_newline() {
    assert_eq!(plain("a\r\nb\rc").unwrap(), "a\nb\nc\n");
    assert_eq!(plain("a\n").unwrap(), "a\n");
    assert_eq!(plain(" \n\t"), Err(Failure::Empty { scripted: false }));
}
