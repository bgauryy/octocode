//! Ecosystems: the contract's names, registry readers and releases.
use super::*;
use octocode_runtime_base::contracts::parsed_contract;
use std::collections::BTreeSet;

fn enum_values() -> Vec<String> {
    let schema = parsed_contract().expect("contract")["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "securityCheck")
        .expect("securityCheck")["querySchema"]
        .to_string();
    let values: serde_json::Value = serde_json::from_str(&schema).expect("schema");
    let mut found = Vec::new();
    collect_ecosystem_enum(&values, &mut found);
    collect_ecosystem_enum(&parsed_contract().expect("contract")["$defs"], &mut found);
    found.sort();
    found.dedup();
    found
}

/// Every enum in the schema that lists the distros: the `ecosystem` values.
fn collect_ecosystem_enum(node: &serde_json::Value, found: &mut Vec<String>) {
    match node {
        serde_json::Value::Object(map) => {
            if let Some(values) = map.get("enum").and_then(|values| values.as_array())
                && values.iter().any(|value| value == "Debian")
            {
                found.extend(values.iter().filter_map(|v| v.as_str().map(str::to_owned)));
            }
            map.values().for_each(|v| collect_ecosystem_enum(v, found));
        }
        serde_json::Value::Array(items) => {
            items.iter().for_each(|v| collect_ecosystem_enum(v, found));
        }
        _ => {}
    }
}

fn every_ecosystem() -> Vec<Ecosystem> {
    enum_values()
        .iter()
        .map(|name| serde_json::from_value(serde_json::json!(name)).expect("an enum value"))
        .collect()
}

/// N2: the contract's registry readers are exactly the ecosystems this
/// module maps to a provider, and every distro and reader is an enum value.
#[test]
fn the_contract_s_readers_are_the_native_reader_set() {
    let contract = security_contract().expect("security block");
    let enum_names: BTreeSet<String> = enum_values().into_iter().collect();
    assert!(!enum_names.is_empty());
    for name in contract
        .ecosystems
        .distros
        .iter()
        .chain(&contract.ecosystems.registry_readers)
    {
        assert!(
            enum_names.contains(name),
            "{name} is not an ecosystem value"
        );
    }
    let native: BTreeSet<String> = every_ecosystem()
        .into_iter()
        .filter(|ecosystem| provider(*ecosystem).is_some())
        .map(|ecosystem| ecosystem.to_string())
        .collect();
    let declared: BTreeSet<String> = contract
        .ecosystems
        .registry_readers
        .iter()
        .cloned()
        .collect();
    assert_eq!(native, declared);
    for ecosystem in every_ecosystem() {
        assert_eq!(
            registry(ecosystem).expect("contract").is_some(),
            declared.contains(&ecosystem.to_string())
        );
    }
}

/// `Display` is the input's spelling; OSV is asked by its own name, which
/// the contract states for each registry artifactSearch reads and which is
/// the same word everywhere else. A release is joined with a colon, and a
/// stated OSV name reads back as the input's spelling, release kept.
#[test]
fn display_is_the_input_spelling_and_osv_is_asked_by_its_own_name() {
    let contract = security_contract().expect("contract");
    let named = &contract.ecosystems.osv_names;
    assert!(!named.is_empty());
    for ecosystem in every_ecosystem() {
        let input = ecosystem.to_string();
        assert!(enum_values().contains(&input));
        let asked = osv(ecosystem);
        assert_eq!(asked, *named.get(&input).unwrap_or(&input), "{input}");
        assert_eq!(osv_name(ecosystem, None), asked);
        assert_eq!(osv_name(ecosystem, Some("r")), format!("{asked}:r"));
        assert_eq!(as_input(&asked), input);
        assert_eq!(as_input(&format!("{asked}:r")), format!("{input}:r"));
        // Only a registry a provider reads has a name of OSV's own.
        if named.contains_key(&input) {
            assert!(provider(ecosystem).is_some(), "{input}");
        }
    }
    // Every registry artifactSearch reads is spelled as artifactSearch does.
    for kind in crate::test_spelling::artifact_ecosystems() {
        let ecosystem = crate::artifact::query::artifact_type::osv_ecosystem(kind);
        assert_eq!(
            ecosystem.to_string(),
            crate::artifact::query::artifact_type::as_str(kind)
        );
        assert!(named.contains_key(&ecosystem.to_string()), "{ecosystem}");
    }
}

#[test]
fn a_release_is_accepted_on_distros_with_releases_and_refused_elsewhere() {
    for ecosystem in every_ecosystem() {
        let outcome = check_release(ecosystem, Some("12"));
        if takes_release(ecosystem).expect("contract") {
            assert!(outcome.is_ok(), "{ecosystem}");
        } else {
            assert_eq!(outcome.expect_err("refused").code, ErrorCode::InvalidInput);
        }
        assert!(check_release(ecosystem, None).is_ok());
    }
}

#[test]
fn rolling_distributions_are_distributions_that_take_no_release() {
    let rolling = &security_contract().expect("contract").ecosystems.rolling;
    assert!(!rolling.is_empty());
    for name in rolling {
        let ecosystem: Ecosystem =
            serde_json::from_value(serde_json::json!(name)).expect("an ecosystem");
        assert!(
            security_contract()
                .expect("contract")
                .is_distro(&ecosystem.to_string()),
            "{ecosystem}"
        );
        assert!(!takes_release(ecosystem).expect("contract"), "{ecosystem}");
        let error = check_release(ecosystem, Some("v1")).expect_err("refused");
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(!error.message.contains("Wolfi,"), "{}", error.message);
    }
}

#[test]
fn registries_round_trip_through_their_ecosystem() {
    for ecosystem in every_ecosystem() {
        if let Some(kind) = provider(ecosystem) {
            assert_eq!(
                crate::artifact::query::artifact_type::osv_ecosystem(kind),
                ecosystem
            );
        }
    }
}
