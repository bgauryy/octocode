use super::*;

#[test]
fn root_help_lists_tools_independent_of_mcp_filters_and_clasify_only_with_a_key() {
    let workspace = Workspace::new();
    for key in ["", "fixture-key"] {
        let output = workspace
            .cli()
            .env("OCTOCODE_ENABLE_LOCAL", "false")
            .env("OCTOCODE_BETA", "false")
            .env("OCTOCODE_ENABLE_CLONE", "false")
            .env("OCTOCODE_CLASSIFICATION_API", key)
            .env("TOOLS_TO_RUN", "localFetch")
            .env("DISABLE_TOOLS", "localFetch,clasify")
            .arg("--help")
            .output()
            .expect("help");
        assert!(output.status.success());
        let text = stdout(&output);
        for tool in octocode_native::id::ToolId::ALL {
            assert_eq!(
                text.contains(&format!("\n  {} ", tool.as_str())),
                match tool {
                    octocode_native::id::ToolId::Clasify => !key.is_empty(),
                    _ => true,
                },
                "{key:?} {}: {text}",
                tool.as_str()
            );
        }
    }
    let output = workspace.cli().arg("--help").output().expect("help");
    let text = stdout(&output);
    assert!(text.contains("schema --view flow"), "{text}");
    // Each tool retains its canonical JSON input and continuation help.
    let own = workspace
        .cli()
        .args(["clasify", "--help"])
        .output()
        .expect("tool help");
    assert!(own.status.success(), "{}", stderr(&own));
    // JSON only: no positional fields to point at, no trailing space.
    let own = stdout(&own);
    assert!(own.contains("{\"queries\":[…]}"), "{own}");
    assert!(own.contains("{\"cursor\":\"…\"}"), "{own}");
    assert!(!own.contains("positional fields"), "{own}");
}

#[test]
fn tool_help_states_each_contract_tools_trigger() {
    let workspace = Workspace::new();
    let contract = octocode_native::contracts::parsed_contract().expect("embedded contract");
    let root = all_tools_enabled(workspace.cli().arg("--help"))
        .output()
        .expect("root help");
    assert!(root.status.success());
    // The commands are exactly the contract tools and the system commands:
    // a tool dropped from the contract must not linger as a command, and
    // none may be missing.
    let system = [
        "schema",
        "config",
        "cache",
        "auth",
        "skill",
        "install",
        "mcp",
        "lsp-server",
        "help",
    ];
    let listed = stdout(&root)
        .split("Commands:")
        .nth(1)
        .expect("commands section")
        .lines()
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter(|line| line.starts_with("  ") && !line.starts_with("   "))
        .filter_map(|line| line.split_whitespace().next())
        .collect::<Vec<_>>();
    for command in system {
        assert!(listed.contains(&command), "missing {command}: {listed:?}");
    }
    let mut listed_tools = listed
        .into_iter()
        .filter(|name| !system.contains(name))
        .collect::<Vec<_>>();
    let mut contract_tools = contract["tools"]
        .as_array()
        .expect("contract tools")
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    listed_tools.sort_unstable();
    contract_tools.sort_unstable();
    assert_eq!(listed_tools, contract_tools);
    let root_text = stdout(&root)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    for tool in contract["tools"].as_array().expect("contract tools") {
        let name = tool["name"].as_str().expect("tool name");
        let trigger = octocode_native::contracts::cli_tool_trigger(name).expect("tool trigger");
        assert!(!trigger.is_empty(), "{name}");
        assert!(
            root_text.contains(trigger),
            "missing {name} trigger in root help: {root_text}"
        );
        for flag in ["-h", "--help"] {
            let output = workspace
                .cli()
                .args([name, flag])
                .output()
                .expect("tool help");
            assert!(output.status.success(), "{name}: {}", stderr(&output));
            let text = stdout(&output)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(text.contains(trigger), "{name} {flag}: {text}");
            assert!(
                !text.contains("MCP beta"),
                "{name} {flag} no longer names the beta gate: {text}"
            );
        }
    }
}

/// A name the help does not list is no command: the call is a usage error.
#[test]
fn a_command_the_help_does_not_list_is_rejected() {
    let workspace = Workspace::new();
    let help = all_tools_enabled(workspace.cli().arg("--help"))
        .output()
        .expect("root help");
    let unlisted = "notACommand";
    assert!(!stdout(&help).contains(unlisted));
    let output = workspace.cli().arg(unlisted).output().expect("unlisted");
    assert_eq!(exit_code(&output), Some(2), "{}", stderr(&output));
}

/// An agent goes from the root help straight to a call: before the output
/// rules, the root help gives each listed tool one entry naming every
/// operation and every field some form requires, and no other word; every
/// form's required fields are also in `<tool> --help`, every form's example
/// validates, and both say where output fields are explained. The expected
/// words come from the contract's variants, never from a copied help text.
#[test]
fn help_names_each_tools_required_fields_and_output_reference() {
    let workspace = Workspace::new();
    let presentation = octocode_native::contracts::cli_presentation().expect("presentation");
    for flag in ["--help", "-h"] {
        let root = all_tools_enabled(workspace.cli().arg(flag))
            .output()
            .expect("root help");
        assert!(root.status.success());
        let root = stdout(&root);
        assert!(
            root.contains("schema <tool> --view output"),
            "{flag}: {root}"
        );
        assert!(
            root.contains("Before a tool's first call: octocode <tool> --help"),
            "{flag}: {root}"
        );
        let (_, section) = root
            .split_once("Required query fields")
            .unwrap_or_else(|| panic!("{flag}: no required-fields section: {root}"));
        if flag == "--help" {
            // Read before the output rules and the command list, not after.
            let at = |text: &str| root.find(text).unwrap_or_else(|| panic!("{text}: {root}"));
            assert!(at("Required query fields") < at("\nOutput: "), "{root}");
            assert!(at("\nOutput: ") < at("\nCommands:"), "{root}");
        }
        for tool in presentation["tools"].as_array().expect("tools") {
            let name = tool["name"].as_str().expect("tool name");
            let forms = tool["views"]["variants"]["variants"]
                .as_array()
                .expect("variants");
            let strings = |value: &serde_json::Value| -> Vec<String> {
                value
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(serde_json::Value::as_str)
                    .map(str::to_owned)
                    .collect()
            };
            let forms: Vec<(Vec<String>, Vec<String>)> = forms
                .iter()
                .map(|form| (strings(&form["operations"]), strings(&form["requires"])))
                .collect();
            // A form that only adds required fields (a view, ranges) to a
            // form of the same operations narrows it: the wider form, a valid
            // first call, stands for it.
            let narrows = |(operations, requires): &(Vec<String>, Vec<String>)| {
                forms.iter().any(|(wider_operations, wider)| {
                    operations.iter().all(|op| wider_operations.contains(op))
                        && wider.len() < requires.len()
                        && wider.iter().all(|field| requires.contains(field))
                })
            };
            let operations: Vec<String> = forms.iter().flat_map(|form| form.0.clone()).collect();
            let required: Vec<String> = forms.iter().flat_map(|form| form.1.clone()).collect();
            let named: Vec<String> = forms
                .iter()
                .filter(|form| !narrows(form))
                .flat_map(|form| form.1.clone())
                .collect();
            // The tool's entry: its line and the deeper-indented lines under it.
            let mut lines = section.lines().take_while(|line| !line.is_empty());
            let head = lines
                .by_ref()
                .find(|line| line.starts_with("  ") && line.split_whitespace().next() == Some(name))
                .unwrap_or_else(|| panic!("{flag}: no required-fields line for {name}: {section}"));
            let entry = std::iter::once(head)
                .chain(lines.take_while(|line| line.starts_with("   ")))
                .collect::<Vec<_>>()
                .join(" ");
            let tokens: Vec<&str> = entry
                .split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|token| !token.is_empty())
                .collect();
            for word in operations.iter().chain(&named) {
                assert!(
                    tokens.contains(&word.as_str()),
                    "{flag}: {name} lacks {word}: {entry}"
                );
            }
            // Nothing beside the contract's words: no alias, no stale field.
            for token in &tokens[1..] {
                let known = operations.iter().chain(&required).any(|word| word == token)
                    || *token == "operation"
                    || (*token == "none" && required.is_empty());
                assert!(
                    known,
                    "{flag}: {name} names {token}, not in its forms: {entry}"
                );
            }
        }
    }
    for tool in presentation["tools"].as_array().expect("tools") {
        let name = tool["name"].as_str().expect("tool name");
        let forms = tool["views"]["variants"]["variants"]
            .as_array()
            .expect("variants");
        let own = all_tools_enabled(workspace.cli().args([name, "--help"]))
            .output()
            .expect("tool help");
        assert!(own.status.success(), "{name}: {}", stderr(&own));
        let own = stdout(&own);
        for form in forms {
            // Each form the root entry summarizes is a valid query.
            octocode_native::contracts::validate_query(name, form["example"].clone())
                .unwrap_or_else(|error| panic!("{name} {} example: {error:?}", form["name"]));
            for field in form["requires"].as_array().expect("requires") {
                let field = field.as_str().expect("field");
                assert!(own.contains(field), "{name} --help lacks {field}: {own}");
            }
        }
        assert!(
            own.contains(&format!("octocode schema {name} --view output")),
            "{name}: {own}"
        );
    }
}
