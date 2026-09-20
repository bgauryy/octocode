mod support;

use support::Workspace;

#[test]
fn scheme_catalog_carries_core_jev_guidance_only_when_available() {
    let workspace = Workspace::new();
    let contract = octocode_native::contracts::parsed_contract().expect("embedded contract");
    let guidance = contract["cliGuidance"]["jev"]
        .as_str()
        .filter(|text| !text.is_empty())
        .expect("core-authored CLI guidance");
    let runtime = workspace.runtime(&[("OCTOCODE_JEV_KEY", "fixture-key".into())]);
    assert_eq!(
        runtime.catalog().expect("native catalog")["cliGuidance"]["jev"],
        guidance
    );
    drop(runtime);

    for enabled in [false, true] {
        let mut command = workspace.cli();
        command.args(["scheme", "--compact"]);
        if enabled {
            command.env("OCTOCODE_JEV_KEY", "fixture-key");
        }
        let output = command.output().expect("scheme command");
        assert!(output.status.success(), "{output:?}");
        let catalog: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("catalog JSON");
        assert_eq!(
            catalog["guidance"]["jev"].as_str() == Some(guidance),
            enabled,
            "{catalog}"
        );
        let jev = catalog["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .find(|tool| tool["name"] == "jev")
            .expect("jev entry");
        assert_eq!(jev["availability"]["enabled"], enabled, "{catalog}");
    }
}
