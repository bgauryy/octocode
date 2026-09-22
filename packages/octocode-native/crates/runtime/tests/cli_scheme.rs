// Integration test crate — assertions use unwrap/expect/panic freely.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

mod support;

use support::Workspace;

#[test]
fn scheme_catalog_is_machine_only_with_availability_scoping() {
    let workspace = Workspace::new();
    for enabled in [false, true] {
        let env = if enabled {
            vec![("OCTOCODE_CLASSIFICATION_API", "fixture-key".into())]
        } else {
            vec![]
        };
        let runtime = workspace.runtime(&env);
        let native_catalog = runtime.catalog().expect("native catalog");
        assert!(
            native_catalog.get("mcpInstructions").is_none(),
            "instructions are core-delivered by the JS layers: {native_catalog}"
        );
        let expected_fingerprint = native_catalog["fingerprint"]
            .as_str()
            .expect("enforcement fingerprint")
            .to_owned();
        drop(runtime);

        let mut command = workspace.cli();
        command.args(["scheme", "--compact"]);
        if enabled {
            command.env("OCTOCODE_CLASSIFICATION_API", "fixture-key");
        }
        let output = command.output().expect("scheme command");
        assert!(output.status.success(), "{output:?}");
        let catalog: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("catalog JSON");
        assert!(catalog.get("instructions").is_none(), "{catalog}");
        assert!(catalog.get("guidance").is_none(), "{catalog}");
        assert_eq!(catalog["fingerprint"], expected_fingerprint, "{catalog}");
        let clasify = catalog["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .find(|tool| tool["name"] == "clasify")
            .expect("clasify entry");
        assert_eq!(clasify["availability"]["enabled"], enabled, "{catalog}");
        let ast_rewrite = catalog["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .find(|tool| tool["name"] == "astRewrite")
            .expect("astRewrite entry");
        assert_eq!(ast_rewrite["availability"]["enabled"], false, "{catalog}");
        assert_eq!(
            ast_rewrite["availability"]["envVar"], "OCTOCODE_BETA",
            "{catalog}"
        );
        let ast_topology = catalog["tools"]
            .as_array()
            .expect("tools array")
            .iter()
            .find(|tool| tool["name"] == "astTopology")
            .expect("astTopology entry");
        assert_eq!(ast_topology["availability"]["enabled"], false, "{catalog}");
        assert_eq!(
            ast_topology["availability"]["envVar"], "OCTOCODE_BETA",
            "{catalog}"
        );
    }
}
