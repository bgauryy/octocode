use super::*;

#[test]
fn families_group_their_server_languages() {
    assert_eq!(family_of("javascriptreact"), Some(TYPESCRIPT_FAMILY));
    assert_eq!(family_of("cuda"), Some(C_FAMILY));
    // Nothing routes these ids to clangd and no grammar reads them.
    assert_eq!(family_of("objective-c"), None);
    assert_eq!(family_of("objective-cpp"), None);
    assert_eq!(family_of("rust"), None);
    assert!(is_typescript_family(Some("typescript")));
    assert!(!is_typescript_family(Some("python")));
    assert!(is_c_family(Some("cuda")));
}
