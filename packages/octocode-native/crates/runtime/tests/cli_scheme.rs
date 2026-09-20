mod support;

use support::Workspace;

#[test]
fn scheme_catalog_carries_availability_scoped_core_instructions() {
    let workspace = Workspace::new();
    for enabled in [false, true] {
        let env = if enabled {
            vec![("OCTOCODE_JEV_KEY", "fixture-key".into())]
        } else {
            vec![]
        };
        let runtime = workspace.runtime(&env);
        let expected = runtime.catalog().expect("native catalog")["mcpInstructions"]
            .as_str()
            .expect("core-authored instructions")
            .to_owned();
        drop(runtime);

        let mut command = workspace.cli();
        command.args(["scheme", "--compact"]);
        if enabled {
            command.env("OCTOCODE_JEV_KEY", "fixture-key");
        }
        let output = command.output().expect("scheme command");
        assert!(output.status.success(), "{output:?}");
        let catalog: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("catalog JSON");
        assert_eq!(catalog["instructions"], expected, "{catalog}");
        assert!(catalog.get("guidance").is_none(), "{catalog}");
        assert_eq!(
            expected.contains("use jev"),
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
