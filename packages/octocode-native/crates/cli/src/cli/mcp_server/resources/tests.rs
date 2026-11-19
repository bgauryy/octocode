use super::{REFERENCES, description};

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
