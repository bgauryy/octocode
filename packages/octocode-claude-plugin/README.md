# Octocode for Claude Code

Octocode brings local and GitHub code research to Claude Code through a local MCP server and all public Octocode skills. Use it to trace an implementation, investigate a regression, compare repositories, review architecture, or follow a documented research workflow. The package includes setup guidance at `/octocode:octocode-get-started`.

## Install

These commands work after the npm package and marketplace catalog have been published:

```sh
claude plugin marketplace add bgauryy/octocode
claude plugin install octocode@octocode
```

Restart your Claude Code session, then run `/octocode:octocode-get-started`. The marketplace fetches `@octocodeai/claude-plugin` from npm. Installing that package with `npm install` alone does not enable a Claude plugin.

Use a current Claude Code release, Node.js compatible with the pinned Octocode runtime, and npm on your PATH. The setup skill reports the exact Node.js requirement. GitHub research also needs GitHub CLI (`gh`) and access to the repositories you request. This distribution targets local Claude Code sessions; local MCP support on other Claude surfaces is not verified.

## GitHub sign-in

Complete browser sign-in in your own terminal:

```sh
gh auth login --hostname github.com --web
gh auth status --active --hostname github.com
```

GitHub CLI owns credential storage. The local Octocode process reads that login internally and sends authenticated requests directly to GitHub. The plugin adds no credential backend or token store. Do not paste tokens into chat or configuration files. GitHub CLI uses the system credential store when available and may fall back to a local file.

Existing environment credentials or an Octocode login take precedence over GitHub CLI. Onboarding checks the effective source without displaying credentials. See [Octocode authentication](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md) for the precedence and enterprise-host configuration.

## Use

Open Claude Code from your project directory. Ask it to research code with Octocode, or invoke `/octocode:octocode-research`. All public skills are included; their scripts and references travel with the plugin. Optional browser tooling, language servers, local model services, and classification services are set up only when their workflows need them. Repository development and beta skills are excluded.

The plugin launches an exact version of `octocode-mcp` with `npx`. npm downloads that runtime and its dependencies when needed. Local tools read the active project; GitHub tools contact GitHub or your configured enterprise host. Selected workflows may fetch repositories, package registries, or public web pages, automate your browser, write requested artifacts, or use a model/classification service you configure. Tool results become part of your Claude conversation. Review each skill's prerequisites before connecting an optional service. No hooks or extra remote MCP services are activated by this package.

## Troubleshooting and updates

- If tools are missing, check `/mcp`, Node.js/npm availability, and the runtime download error, then restart the session.
- If local reads are denied, open the intended project and check its workspace configuration. Do not point the workspace at the plugin cache or allow the whole filesystem.
- If GitHub access fails, check the active account and repository permissions with `gh auth status`. Do not display the token.
- If you installed the same skills separately, choose which installation to keep to avoid duplicate triggers.

```sh
claude plugin update octocode@octocode
claude plugin disable octocode@octocode
claude plugin uninstall octocode@octocode
```

Updates from a third-party marketplace are not automatically enabled by default. Uninstalling this plugin does not sign you out of GitHub CLI. Report issues at [Octocode issues](https://github.com/bgauryy/octocode/issues). Packaging and publication instructions are in the repository's [architecture guide](https://github.com/bgauryy/octocode/blob/main/packages/octocode-claude-plugin/ARCHITECTURE.md).
