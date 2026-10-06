//! Every supported MCP client, one row each; install and agent management
//! derive all per-client behavior from `CLIENTS`.
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum ConfigFormat {
    Json,
    Toml,
    Yaml,
}

/// Layout of the Octocode server entry a client reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum EntryShape {
    /// `{command, type: "stdio", args, env}`.
    Stdio,
    /// `{command, args, env}` with no transport type.
    Untyped,
    /// `{type: "local", command: [runner, ...args], environment}`.
    Opencode,
    /// `{cmd, args, type: "stdio", name, enabled, envs}`.
    Goose,
}

impl EntryShape {
    pub(super) fn env_key(self) -> &'static str {
        match self {
            Self::Opencode => "environment",
            Self::Goose => "envs",
            Self::Stdio | Self::Untyped => "env",
        }
    }

    pub(super) fn command_key(self) -> &'static str {
        match self {
            Self::Goose => "cmd",
            Self::Stdio | Self::Untyped | Self::Opencode => "command",
        }
    }
}

/// The boolean a client reads to switch an entry off.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum EnableFlag {
    Enabled,
    Disabled,
}

pub(super) struct ClientSpec {
    pub(super) id: &'static str,
    pub(super) aliases: &'static [&'static str],
    pub(super) format: ConfigFormat,
    pub(super) shape: EntryShape,
    /// Map that holds the `octocode` entry.
    pub(super) servers: &'static str,
    /// Enable flag verified against the client's published configuration docs.
    pub(super) enable: Option<EnableFlag>,
    /// Workspace config, relative to the working directory.
    pub(super) workspace: Option<&'static str>,
    /// Per-project entries under `projects.<cwd>` of the home config.
    pub(super) local: bool,
    /// Env var that moves the home config somewhere we cannot resolve safely.
    pub(super) config_dir_env: Option<&'static str>,
    home: fn(&Dirs) -> PathBuf,
}

struct Dirs {
    home: PathBuf,
    app_support: PathBuf,
    config: PathBuf,
}

impl Dirs {
    fn vscode_storage(&self) -> PathBuf {
        self.app_support
            .join("Code")
            .join("User")
            .join("globalStorage")
    }
}

pub(super) static CLIENTS: [ClientSpec; 15] = [
    ClientSpec {
        workspace: Some(".cursor/mcp.json"),
        ..ClientSpec::stdio("cursor", |dirs| dirs.home.join(".cursor").join("mcp.json"))
    },
    ClientSpec {
        aliases: &["claude"],
        ..ClientSpec::stdio("claude-desktop", |dirs| {
            dirs.app_support
                .join("Claude")
                .join("claude_desktop_config.json")
        })
    },
    ClientSpec {
        workspace: Some(".mcp.json"),
        local: true,
        config_dir_env: Some("CLAUDE_CONFIG_DIR"),
        ..ClientSpec::stdio("claude-code", |dirs| dirs.home.join(".claude.json"))
    },
    ClientSpec::stdio("windsurf", |dirs| {
        dirs.home
            .join(".codeium")
            .join("windsurf")
            .join("mcp_config.json")
    }),
    ClientSpec::stdio("trae", |dirs| {
        dirs.app_support.join("Trae").join("mcp.json")
    }),
    ClientSpec::stdio("antigravity", |dirs| {
        dirs.home
            .join(".gemini")
            .join("antigravity")
            .join("mcp_config.json")
    }),
    ClientSpec {
        aliases: &["vscode"],
        enable: Some(EnableFlag::Disabled),
        ..ClientSpec::stdio("vscode-cline", |dirs| {
            dirs.vscode_storage()
                .join("saoudrizwan.claude-dev")
                .join("settings")
                .join("cline_mcp_settings.json")
        })
    },
    ClientSpec {
        enable: Some(EnableFlag::Disabled),
        ..ClientSpec::stdio("vscode-roo", |dirs| {
            dirs.vscode_storage()
                .join("rooveterinaryinc.roo-cline")
                .join("settings")
                .join("mcp_settings.json")
        })
    },
    ClientSpec {
        workspace: Some(".continue/mcpServers/octocode.json"),
        ..ClientSpec::stdio("vscode-continue", |dirs| {
            dirs.home
                .join(".continue")
                .join("mcpServers")
                .join("octocode.json")
        })
    },
    ClientSpec {
        shape: EntryShape::Untyped,
        servers: "context_servers",
        ..ClientSpec::stdio("zed", |dirs| dirs.config.join("zed").join("settings.json"))
    },
    ClientSpec {
        shape: EntryShape::Opencode,
        servers: "mcp",
        enable: Some(EnableFlag::Enabled),
        ..ClientSpec::stdio("opencode", |dirs| {
            let plain = dirs.config.join("opencode").join("opencode.json");
            let commented = plain.with_extension("jsonc");
            if !plain.exists() && commented.exists() {
                commented
            } else {
                plain
            }
        })
    },
    ClientSpec::stdio("gemini-cli", |dirs| {
        dirs.home.join(".gemini").join("settings.json")
    }),
    ClientSpec {
        enable: Some(EnableFlag::Disabled),
        workspace: Some(".kiro/settings/mcp.json"),
        ..ClientSpec::stdio("kiro", |dirs| {
            dirs.home.join(".kiro").join("settings").join("mcp.json")
        })
    },
    ClientSpec {
        format: ConfigFormat::Toml,
        shape: EntryShape::Untyped,
        servers: "mcp_servers",
        enable: Some(EnableFlag::Enabled),
        workspace: Some(".codex/config.toml"),
        ..ClientSpec::stdio("codex", |dirs| {
            std::env::var_os("CODEX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| dirs.home.join(".codex"))
                .join("config.toml")
        })
    },
    ClientSpec {
        format: ConfigFormat::Yaml,
        shape: EntryShape::Goose,
        servers: "extensions",
        enable: Some(EnableFlag::Enabled),
        ..ClientSpec::stdio("goose", |dirs| {
            if cfg!(windows) {
                dirs.app_support
                    .join("Block")
                    .join("goose")
                    .join("config")
                    .join("config.yaml")
            } else {
                dirs.config.join("goose").join("config.yaml")
            }
        })
    },
];

/// The client named by an id or alias.
pub(super) fn client(name: &str) -> Option<&'static ClientSpec> {
    CLIENTS
        .iter()
        .find(|spec| spec.id == name || spec.aliases.contains(&name))
}

impl ClientSpec {
    const fn stdio(id: &'static str, home: fn(&Dirs) -> PathBuf) -> Self {
        Self {
            id,
            aliases: &[],
            format: ConfigFormat::Json,
            shape: EntryShape::Stdio,
            servers: "mcpServers",
            enable: None,
            workspace: None,
            local: false,
            config_dir_env: None,
            home,
        }
    }

    pub(super) fn home_path(&self) -> Option<PathBuf> {
        let home = std::env::home_dir()?;
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        Some((self.home)(&Dirs {
            app_support: app_support_dir(&home),
            home,
            config,
        }))
    }

    pub(super) fn scopes(&self) -> impl Iterator<Item = &'static str> {
        std::iter::once("home")
            .chain(self.workspace.map(|_| "workspace"))
            .chain(self.local.then_some("local"))
    }

    pub(super) fn config_dir_overridden(&self) -> bool {
        self.config_dir_env
            .is_some_and(|name| std::env::var_os(name).is_some())
    }

    /// `[server map, "octocode"]`.
    pub(super) fn server_keys(&self) -> Vec<String> {
        vec![self.servers.to_owned(), "octocode".to_owned()]
    }

    /// OpenCode's V2 layout nests servers under `mcp.servers` with a `disabled` flag.
    pub(super) fn nested_servers(&self, root: &Value) -> bool {
        self.shape == EntryShape::Opencode && root.pointer("/mcp/servers").is_some()
    }
}

fn app_support_dir(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"))
    }
}
