use super::compose_instructions;
use crate::contracts::cli_presentation;
use crate::contracts::parsed_contract;
use serde_json::Value;

/// Core's own composed text for each enabled-tool set, generated with the contract.
const FIXTURES: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../octocode-config/contract/instruction-fixtures.json"
));

fn file() -> Value {
    serde_json::from_str(FIXTURES).expect("instruction fixtures")
}

fn fixtures() -> Vec<Value> {
    file()["sets"].as_array().expect("sets").clone()
}

#[test]
fn composition_equals_core_for_every_enabled_set() {
    let presentation = cli_presentation().expect("presentation");
    let fixtures = fixtures();
    assert!(
        fixtures.len() > 4,
        "all on, none optional, local, github, and each block"
    );
    for fixture in &fixtures {
        let enabled: Vec<&str> = fixture["enabled"]
            .as_array()
            .expect("enabled")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let composed = compose_instructions(presentation, |name| enabled.contains(&name));
        assert_eq!(
            composed,
            fixture["text"].as_str().expect("text"),
            "{}",
            fixture["name"]
        );
    }
}

#[test]
fn the_enabled_sets_name_only_tools_of_the_contract() {
    let contract = parsed_contract().expect("contract");
    let known: Vec<&str> = contract["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect();
    for fixture in fixtures() {
        for name in fixture["enabled"].as_array().expect("enabled") {
            assert!(known.contains(&name.as_str().expect("name")), "{name}");
        }
    }
}

/// Every enabled set, all on included, stays within core's host cut,
/// which the generator carries beside the sets.
#[test]
fn every_set_stays_within_the_core_ceiling() {
    let file = file();
    let ceiling = file["coreChars"]
        .as_u64()
        .expect("MCP_INSTRUCTION_CORE_CHARS");
    let presentation = cli_presentation().expect("presentation");
    let all_on = compose_instructions(presentation, |_| true);
    assert!(all_on.chars().count() as u64 <= ceiling, "{}", all_on.len());
    for fixture in fixtures() {
        let text = fixture["text"].as_str().expect("text");
        assert!(
            text.chars().count() as u64 <= ceiling,
            "{}",
            fixture["name"]
        );
    }
}
