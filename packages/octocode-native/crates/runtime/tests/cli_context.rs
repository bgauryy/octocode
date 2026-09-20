mod support;

use support::Workspace;

#[test]
fn context_modes_use_core_jev_guidance_only_when_available() {
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

    for mode in [None, Some("--minimal")] {
        for enabled in [false, true] {
            let mut command = workspace.cli();
            command.arg("context");
            if let Some(mode) = mode {
                command.arg(mode);
            }
            if enabled {
                command.env("OCTOCODE_JEV_KEY", "fixture-key");
            }
            let output = command.output().expect("context command");
            assert!(output.status.success(), "{output:?}");
            let text = String::from_utf8(output.stdout).expect("context UTF-8");
            assert_eq!(text.contains(guidance), enabled, "{text}");
            if mode.is_none() {
                assert!(text.contains("  Reasoning:"), "{text}");
                assert!(
                    text.contains(if enabled {
                        "    jev —"
                    } else {
                        "    jev [disabled] —"
                    }),
                    "{text}"
                );
            }
        }
    }
}
