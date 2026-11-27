use super::ToolArgs;
use clap::CommandFactory;

#[test]
fn tool_calls_have_one_json_language() {
    for tool in octocode_runtime_base::id::ToolId::ALL {
        for json in [r#"{"queries":[{}]}"#, r#"{"cursor":"issued"}"#] {
            assert!(
                super::super::Args::command()
                    .try_get_matches_from(["octocode", tool.as_str(), json])
                    .is_ok()
            );
            // One envelope per call: a second one, or a file beside it,
            // is a parse error.
            for second in [[json, json], ["--input", "query.json"]] {
                let args = ["octocode", tool.as_str(), second[0], second[1], json];
                assert!(
                    super::super::Args::command()
                        .try_get_matches_from(args)
                        .is_err(),
                    "{args:?}"
                );
            }
        }
    }
}

/// A tool command is read only from a contract tool's subcommand: none, or
/// one the contract does not name, is a parse error; an update reads
/// the same way, with the update parser listing every tool.
#[test]
fn a_tool_command_is_read_only_from_a_contract_tools_subcommand() {
    use super::ToolCommand;
    use clap::{FromArgMatches, Subcommand};
    let matches = |command: clap::Command, args: &[&str]| {
        command
            .allow_external_subcommands(true)
            .try_get_matches_from(args)
            .expect("parsed")
    };
    let none = matches(clap::Command::new("octocode"), &["octocode"]);
    assert_eq!(
        ToolCommand::from_arg_matches(&none)
            .expect_err("no tool")
            .kind(),
        clap::error::ErrorKind::MissingSubcommand
    );
    let unknown = matches(clap::Command::new("octocode"), &["octocode", "noSuchTool"]);
    assert_eq!(
        ToolCommand::from_arg_matches(&unknown)
            .expect_err("not a tool")
            .kind(),
        clap::error::ErrorKind::InvalidSubcommand
    );
    let parser = ToolCommand::augment_subcommands_for_update(clap::Command::new("octocode"));
    for tool in octocode_runtime_base::id::ToolId::ALL {
        assert!(parser.find_subcommand(tool.as_str()).is_some(), "{tool:?}");
    }
    let local = ToolCommand::augment_subcommands(clap::Command::new("octocode"))
        .try_get_matches_from(["octocode", "localSearch", "{}"])
        .expect("tool call");
    let mut command = ToolCommand::from_arg_matches(&local).expect("tool");
    assert_eq!(command.name, "localSearch");
    command
        .update_from_arg_matches(&local)
        .expect("update reads the same call");
    assert_eq!(command.args.json_arg(), Some("{}"));
}

/// A query file saved with a UTF-8 byte-order mark reads as its JSON.
#[test]
fn an_input_file_with_a_byte_order_mark_reads_as_its_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("query.json");
    std::fs::write(&file, "\u{feff}{\"queries\":[]}").expect("query file");
    let args = ToolArgs {
        args: Vec::new(),
        input: Some(file),
        content_stdin: false,
        all_pages: false,
    };
    assert_eq!(
        args.query_text().expect("readable"),
        Some("{\"queries\":[]}".to_owned())
    );
}
