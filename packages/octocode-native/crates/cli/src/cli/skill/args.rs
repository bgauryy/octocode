use clap::{Args, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub(crate) enum SkillCommand {
    /// List the skills embedded in this executable and their installation status.
    List,
    /// Show a bundled skill's instructions and environment requirements.
    Info { name: String },
    /// Install bundled skills, or a local skill with --add.
    Install(InstallArgs),
    /// Remove installations; edited copies require --force.
    Remove(RemoveArgs),
    /// Check freshness, links and environment; optionally repair existing installations.
    Check(CheckArgs),
}

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub(crate) enum Mode {
    #[default]
    Symlink,
    Copy,
    Auto,
}

#[derive(Debug, Args)]
pub(crate) struct InstallArgs {
    #[arg(conflicts_with = "all")]
    pub names: Vec<String>,
    /// Install every bundled skill.
    #[arg(long, conflicts_with = "add")]
    pub all: bool,
    /// Import a local directory or SKILL.md; an optional name overrides its folder name.
    #[arg(long, value_name = "PATH")]
    pub add: Option<PathBuf>,
    /// Comma-separated platforms: pi,cursor,claude,codex,opencode,copilot,gemini,all.
    #[arg(long, requires = "scope", conflicts_with = "path")]
    pub platform: Option<String>,
    /// Link or copy into each platform's user directory.
    #[arg(long, group = "scope", requires = "platform")]
    pub global: bool,
    /// Link or copy into an existing project directory.
    #[arg(long, group = "scope", requires = "platform", value_name = "DIR")]
    pub project_dir: Option<PathBuf>,
    /// Install to a custom canonical directory without platform links.
    #[arg(long, conflicts_with_all = ["platform", "global", "project_dir"], value_name = "DIR")]
    pub path: Option<PathBuf>,
    #[arg(long, value_enum, default_value = "symlink")]
    pub mode: Mode,
    /// Replace conflicts, including edited copies.
    #[arg(long)]
    pub force: bool,
    /// Refresh the canonical skill and unedited managed copies.
    #[arg(long)]
    pub upgrade: bool,
    /// Report actions and conflicts without writing files.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RemoveArgs {
    #[arg(conflicts_with = "all")]
    pub names: Vec<String>,
    #[arg(long)]
    pub all: bool,
    /// Restrict removal to these global platform directories.
    #[arg(long)]
    pub platform: Option<String>,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct CheckArgs {
    pub names: Vec<String>,
    #[arg(long)]
    pub platform: Option<String>,
    /// Also repair the current workspace's .agents/skills links.
    #[arg(long)]
    pub workspace: bool,
    #[arg(long)]
    pub fix: bool,
    #[arg(long, requires = "fix")]
    pub dry_run: bool,
    #[arg(long)]
    pub no_env: bool,
}
