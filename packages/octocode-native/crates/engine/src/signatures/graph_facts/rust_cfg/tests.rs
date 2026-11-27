use super::cfg_requires_test;

#[test]
fn only_test_only_predicates_require_test() {
    for gated in [
        "cfg(test)",
        "cfg( test )",
        "cfg(all(test, feature = \"x\"))",
        "cfg(any(test, all(test, unix)))",
    ] {
        assert!(cfg_requires_test(gated), "{gated}");
    }
    for open in [
        "cfg(not(test))",
        "cfg(any(test, feature = \"x\"))",
        "cfg(feature = \"test\")",
        "cfg(unix)",
        "cfg_attr(test, allow(dead_code))",
        "allow(dead_code)",
        "cfg()",
    ] {
        assert!(!cfg_requires_test(open), "{open}");
    }
}
