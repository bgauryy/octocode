use super::*;

#[test]
fn admission_keys_on_the_last_segment_and_a_known_qualifier() {
    assert_eq!(admit("ready"), Some(MacroArgs::Forwarding));
    assert_eq!(admit("std::task::ready"), Some(MacroArgs::Forwarding));
    assert_eq!(admit("::std::vec"), Some(MacroArgs::Forwarding));
    assert_eq!(admit("core::assert_eq"), Some(MacroArgs::Forwarding));
    assert_eq!(admit("matches"), Some(MacroArgs::Scrutinee));
    assert_eq!(admit("tokio::ready"), None);
    assert_eq!(admit("tracing::info"), None);
    assert_eq!(admit("async_assert_fn"), None);
    assert_eq!(admit("quote"), None);
    assert_eq!(admit("stringify"), None);
    assert_eq!(admit("assert_ready"), None);
}

#[test]
fn only_bare_names_are_shadowed() {
    let chain = |paths: &[&str]| paths.iter().map(|p| (*p).to_owned()).collect::<Vec<_>>();
    assert!(shadowed(
        &chain(&["assert", "ready"]),
        ["ready"].into_iter()
    ));
    assert!(!shadowed(
        &chain(&["std::task::ready"]),
        ["ready"].into_iter()
    ));
    assert!(!shadowed(&chain(&["ready"]), ["other"].into_iter()));
}

#[test]
fn only_std_paths_bind_an_admitted_name() {
    assert!(!foreign_use("std::task::ready"));
    assert!(!foreign_use("futures::ready"));
    assert!(foreign_use("my_ready"));
    assert!(foreign_use("crate::m::ready"));
    assert!(foreign_use("pretty_assertions::assert_eq"));
}
