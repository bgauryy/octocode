use super::{REFERENCES, TOOL_FLOW_PARTS, description, joined, tool_flow};

/// Every served reference states core's description, none a native copy.
#[test]
fn every_reference_takes_its_description_from_core() {
    let presentation =
        octocode_runtime_base::contracts::cli_presentation().expect("embedded presentation");
    let described = presentation["agentText"]["resources"]
        .as_object()
        .expect("agentText.resources");
    for reference in &REFERENCES {
        let text = description(reference.name);
        assert!(!text.is_empty(), "{}", reference.name);
        assert_eq!(
            described[reference.name],
            text.as_str(),
            "{}",
            reference.name
        );
    }
    assert_eq!(described.len(), REFERENCES.len());
}

/// The guide is every part's sections, in order, and no part's opening line:
/// that line links sibling files, which the guide serves as its own text.
#[test]
fn the_tool_flow_guide_is_its_parts_without_their_opening_lines() {
    let guide = tool_flow();
    let mut rest = guide;
    for part in TOOL_FLOW_PARTS {
        let (opening, sections) = part
            .split_once("\n\n")
            .expect("opening line, then sections");
        assert!(!opening.contains('\n'), "{opening}");
        assert!(!guide.contains(opening), "{opening}");
        rest = rest
            .trim_start_matches('\n')
            .strip_prefix(sections)
            .unwrap_or_else(|| panic!("out of order: {opening}"));
    }
    assert!(rest.is_empty(), "{rest}");
    assert_eq!(joined(&["opening\n\na\n", "opening\n\nb\n"]), "a\n\nb\n");
}

/// Every sibling file a served reference links is served too: a reader
/// resolves the link against the reference's URI.
#[test]
fn every_sibling_link_in_a_served_reference_is_served() {
    for reference in &REFERENCES {
        let text = (reference.text)();
        for target in text
            .split("](")
            .skip(1)
            .filter_map(|rest| rest.split(')').next())
        {
            let file = target.split('#').next().unwrap_or_default();
            if file.is_empty() || file.contains('/') {
                continue;
            }
            let uri = format!("octocode://research/{file}");
            assert!(
                REFERENCES.iter().any(|served| served.uri == uri),
                "{} links {file}, which is not served",
                reference.name
            );
        }
    }
}
