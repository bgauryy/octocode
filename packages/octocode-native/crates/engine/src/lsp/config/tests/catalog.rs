use super::*;

#[test]
fn server_languages_keep_catalog_order_and_drop_unresolved_servers() {
    use crate::lsp::config::{LspLanguageServerConfig, catalog::server_languages_with};
    let config = |command: &str| LspLanguageServerConfig {
        command: command.to_owned(),
        args: Some(Vec::new()),
        workspace_root: "/w".to_owned(),
        language_id: None,
        initialization_options: None,
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    let languages = server_languages_with(
        |extension| match extension {
            ".java" => Some(config("jdtls")),
            ".rs" => Some(config("rust-analyzer")),
            ".ts" => Some(config("node")),
            ".go" => Some(config("gopls")),
            _ => None,
        },
        |config| config.command != "gopls",
    );
    assert_eq!(languages, vec!["ts/js", "rust", "java"]);
}

#[cfg(unix)]
#[test]
fn rustup_proxy_counts_only_when_a_toolchain_ships_the_component() {
    use crate::lsp::config::probe::rustup_proxy_component_installed;
    use std::os::unix::fs::PermissionsExt;
    let base = TmpDir::new();
    let bin = base.join("cargo/bin");
    let home = base.join("rustup");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(home.join("toolchains/stable/bin")).unwrap();
    let executable = |path: &std::path::Path| {
        std::fs::write(path, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    };
    executable(&bin.join("rustup"));
    let proxy = bin.join("rust-analyzer");
    std::os::unix::fs::symlink("rustup", &proxy).unwrap();
    assert!(!rustup_proxy_component_installed(&proxy, Some(&home)));
    assert!(!rustup_proxy_component_installed(&proxy, None));
    executable(&home.join("toolchains/stable/bin/rust-analyzer"));
    assert!(rustup_proxy_component_installed(&proxy, Some(&home)));
    // A real (non-proxy) binary is the server itself.
    let real = base.join("real-rust-analyzer");
    executable(&real);
    assert!(rustup_proxy_component_installed(&real, None));
}

#[test]
fn available_server_languages_use_known_labels() {
    let first =
        crate::lsp::config::available_server_languages(".", &LspDiscoveryOptions::default());
    for label in &first {
        assert!(
            ["ts/js", "py", "rust", "c/c++", "go", "c#", "java"].contains(label),
            "{label}"
        );
    }
}

#[test]
fn node_launcher_never_treats_the_native_octocode_binary_as_node() {
    assert!(is_node_executable(std::path::Path::new(
        "/usr/local/bin/node"
    )));
    assert!(is_node_executable(std::path::Path::new("C:/node.exe")));
    assert!(!is_node_executable(std::path::Path::new(
        "/usr/local/bin/octocode"
    )));
}

#[test]
fn routes_scala_to_metals_without_spurious_stdio_arguments() {
    for extension in [".scala", ".sc", ".sbt"] {
        let spec =
            crate::lsp::config::catalog::spec_for_extension(extension).expect("Scala provider");
        assert_eq!(spec.language_id, "scala");
        assert_eq!(spec.command, "metals");
        assert!(spec.args.is_empty());
    }
}

#[test]
fn detects_protocol_language_ids_with_grammar_fallback() {
    #[cfg(not(feature = "tree-sitter-asm"))]
    for file_name in ["demo.asm", "demo.assembly", "demo.S"] {
        assert_eq!(detect_language_id(file_name), None, "{file_name}");
    }
    let cases = [
        ("demo.ts", "typescript"),
        ("demo.tsx", "typescriptreact"),
        ("demo.js", "javascript"),
        ("demo.jsx", "javascriptreact"),
        ("demo.JSX", "javascriptreact"),
        ("demo.py", "python"),
        ("demo.go", "go"),
        ("demo.rs", "rust"),
        ("demo.java", "java"),
        ("demo.c", "c"),
        ("demo.cpp", "cpp"),
        ("demo.cu", "cuda"),
        ("demo.cuh", "cuda"),
        #[cfg(feature = "tree-sitter-asm")]
        ("demo.asm", "asm"),
        #[cfg(feature = "tree-sitter-asm")]
        ("demo.assembly", "asm"),
        #[cfg(feature = "tree-sitter-asm")]
        ("demo.S", "asm"),
        ("demo.cs", "csharp"),
        ("demo.scala", "scala"),
        ("demo.sbt", "scala"),
    ];

    for (file_name, expected) in cases {
        assert_eq!(
            detect_language_id(file_name).as_deref(),
            Some(expected),
            "{file_name}"
        );
    }
}

#[test]
fn builtin_routes_intersect_enabled_grammars_exactly() {
    // Every enabled grammar routes to a built-in server except assembly,
    // which claims none.
    let mut expected: Vec<_> = crate::signatures::languages::supported_extensions()
        .into_iter()
        .filter(|extension| {
            crate::signatures::languages::find_entry(extension)
                .is_some_and(|entry| entry.language_id != Some("asm"))
        })
        .collect();
    expected.sort_unstable();
    let mut actual: Vec<_> = crate::signatures::languages::supported_extensions()
        .into_iter()
        .filter(|extension| {
            crate::lsp::config::catalog::spec_for_extension(&format!(".{extension}")).is_some()
        })
        .collect();
    actual.sort_unstable();
    assert_eq!(actual, expected, "built-in LSP route inventory drift");
}

#[test]
fn only_rust_and_go_servers_take_options_as_a_configuration_section() {
    assert_eq!(
        crate::lsp::config::configuration_section(Some("rust")),
        Some("rust-analyzer")
    );
    assert_eq!(
        crate::lsp::config::configuration_section(Some("go")),
        Some("gopls")
    );
    for language_id in ["typescript", "python", "java", "cpp", "unknown"] {
        assert_eq!(
            crate::lsp::config::configuration_section(Some(language_id)),
            None,
            "{language_id}"
        );
    }
    assert_eq!(crate::lsp::config::configuration_section(None), None);
}

#[test]
fn all_cpp_extensions_resolve_to_clangd() {
    for extension in ["cpp", "cc", "cxx", "hpp", "hh", "hxx"] {
        let config = default_server_for_file(
            &format!("fixture.{extension}"),
            "/workspace",
            &LspDiscoveryOptions::default(),
        )
        .expect(extension);
        assert_eq!(config.command, "clangd");
        assert_eq!(config.language_id.as_deref(), Some("cpp"));
    }
}

#[test]
fn all_cuda_extensions_resolve_to_clangd() {
    for extension in ["cu", "cuh"] {
        let config = default_server_for_file(
            &format!("fixture.{extension}"),
            "/workspace",
            &LspDiscoveryOptions::default(),
        )
        .expect(extension);
        assert_eq!(config.command, "clangd");
        assert_eq!(config.language_id.as_deref(), Some("cuda"));
    }
}

/// A `.cuh` with no compile command is a CUDA *header* to clang's
/// fallback command, which plans no job for it (`invalid AST`); CUDA
/// files get the language and a host-only target as fallback flags, and
/// the other C-family files are served as before.
#[test]
fn cuda_files_get_clangd_fallback_flags_and_other_c_family_files_do_not() {
    let options = LspDiscoveryOptions::default();
    for extension in ["cu", "cuh"] {
        let config =
            default_server_for_file(&format!("fixture.{extension}"), "/workspace", &options)
                .expect(extension);
        assert_eq!(
            config.initialization_options,
            Some(json!({ "fallbackFlags": ["-xcuda", "--cuda-host-only"] })),
            "{extension}"
        );
    }
    for extension in ["c", "h", "cpp", "hpp"] {
        let config =
            default_server_for_file(&format!("fixture.{extension}"), "/workspace", &options)
                .expect(extension);
        assert_eq!(config.initialization_options, None, "{extension}");
    }
}

#[test]
fn a_users_clangd_fallback_flags_replace_the_cuda_default() {
    let mut config = LspLanguageServerConfig {
        command: "/usr/bin/clangd-18".to_owned(),
        args: None,
        workspace_root: "/workspace".to_owned(),
        language_id: Some("cuda".to_owned()),
        initialization_options: Some(
            json!({ "fallbackFlags": ["-xcuda", "--cuda-path=/opt/cuda"] }),
        ),
        env: None,
        max_memory_mb: None,
        source: Default::default(),
    };
    crate::lsp::config::apply_server_default_options(&mut config, None);
    crate::lsp::config::apply_server_default_options(&mut config, None);
    assert_eq!(
        config.initialization_options,
        Some(json!({ "fallbackFlags": ["-xcuda", "--cuda-path=/opt/cuda"] }))
    );
}

#[test]
fn only_clangd_declares_project_config_files() {
    assert_eq!(
        crate::lsp::config::project_config_files("clangd"),
        crate::lsp::config::project_config_files("/usr/bin/clangd-18")
    );
    assert!(crate::lsp::config::project_config_files("clangd").contains(&"compile_flags.txt"));
    assert!(crate::lsp::config::project_config_files("clangd").contains(&".clangd"));
    assert!(crate::lsp::config::project_config_files("clangd").contains(&"compile_commands.json"));
    assert!(crate::lsp::config::project_config_files("rust-analyzer").is_empty());
    assert!(crate::lsp::config::project_config_files("typescript-language-server").is_empty());
}

#[test]
fn assembly_server_requires_user_config() {
    for extension in ["asm", "assembly", "s", "S"] {
        assert!(
            default_server_for_file(
                &format!("fixture.{extension}"),
                "/workspace",
                &LspDiscoveryOptions::default(),
            )
            .is_none(),
            ".{extension} must not auto-launch a server"
        );
    }

    // Retired `OCTOCODE_ASM_SERVER_PATH` launches nothing.
    let options = LspDiscoveryOptions {
        env: BTreeMap::from([(
            "OCTOCODE_ASM_SERVER_PATH".to_owned(),
            "/opt/asm-lsp".to_owned(),
        )]),
        ..LspDiscoveryOptions::default()
    };
    assert!(default_server_for_file("fixture.asm", "/workspace", &options).is_none());
}

#[test]
fn removed_languages_have_no_builtin_server() {
    for extension in [
        "sh", "php", "json", "yaml", "html", "css", "scss", "less", "sql", "swift", "rb", "kt",
        "ex",
    ] {
        assert!(
            default_server_for_file(
                &format!("fixture.{extension}"),
                "/workspace",
                &LspDiscoveryOptions::default(),
            )
            .is_none(),
            ".{extension} must require trusted custom configuration"
        );
    }
}

#[test]
fn typescript_language_server_remains_the_stable_default() {
    let spec = crate::lsp::config::catalog::spec_for_extension(".ts").expect("TypeScript provider");
    assert_eq!(spec.command, "typescript-language-server");
    assert_eq!(spec.args, ["--stdio"]);
}

#[test]
fn detects_rust_analyzer_command_names() {
    assert!(is_rust_analyzer_command("rust-analyzer"));
    assert!(is_rust_analyzer_command("/usr/local/bin/rust-analyzer"));
    assert!(!is_rust_analyzer_command("typescript-language-server"));
}
