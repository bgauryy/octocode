use super::RuntimeLimits;

/// The automatic page size is core's `limits`, read from the embedded
/// contract: a change in core changes native behavior, with no native
/// copy of the value.
#[test]
fn page_default_is_the_contract_limit() {
    let contract: serde_json::Value =
        serde_json::from_str(crate::contracts::contract_json()).expect("embedded contract");
    let limit = |name: &str| {
        contract["limits"][name]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or_else(|| panic!("contract limits.{name}"))
    };
    assert_eq!(
        RuntimeLimits::DEFAULT.auto_page_chars,
        limit("defaultResponseChars")
    );
}
