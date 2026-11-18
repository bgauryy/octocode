#[test]
fn the_built_in_provider_needs_no_setting() {
    assert!(
        crate::config::CONFIG_FIELDS
            .iter()
            .all(|field| !field.path.starts_with("classification.type")),
        "no setting selects the provider"
    );
    assert!(!super::provider().defaults().host.is_empty());
}
