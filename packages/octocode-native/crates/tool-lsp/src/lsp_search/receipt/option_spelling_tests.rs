use super::RustAnalysis;
use octocode_test_support::json_fixture;

/// The receipt reads a setting as rust-analyzer's configuration lookup
/// does: a flat dotted key counts as much as the nested path.
#[test]
fn either_spelling_of_a_setting_is_what_the_server_runs() {
    for options in [
        json_fixture(
            r#"{
                    "cargo": {"buildScripts": {"enable": true}},
                    "procMacro": {"enable": true},
                    "cargo.features": "all"
                }"#,
        ),
        json_fixture(
            r#"{
                    "cargo.buildScripts.enable": true,
                    "procMacro.enable": true,
                    "cargo": {"features": "all"}
                }"#,
        ),
    ] {
        let analysis = RustAnalysis::from_options(Some(&options));
        assert!(analysis.build_scripts, "{options}");
        assert!(analysis.proc_macros, "{options}");
        assert!(analysis.all_features, "{options}");
        assert!(analysis.validate().is_ok(), "{options}");
    }
    let flat_off = json_fixture(
        r#"{
                "cargo.buildScripts.enable": false,
                "cargo": {"buildScripts": {"enable": true}}
            }"#,
    );
    assert!(
        !RustAnalysis::from_options(Some(&flat_off)).build_scripts,
        "the flat key is read first, as the server reads it"
    );
}
