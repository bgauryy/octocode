# Configure Octocode

## Manual MCP configuration

For a client that accepts `mcpServers`, add the following entry and restart the client. Set the server working directory to the project when the client supports it.

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp"]
    }
  }
}
```

With the standalone binary, set `command` to `octocode` and `args` to `["mcp"]`. If the client starts elsewhere, its server environment can set `WORKSPACE_ROOT` to the project’s absolute path for local reads. Project settings still come from the launch directory. [MCP setup and troubleshooting](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_MCP.md)

## Configuration and folders

| Location | What it controls |
|---|---|
| Process environment or the MCP entry’s `env` | Settings for that process. |
| `<launch directory>/.octocode/` | Project `.env` and `.octocoderc` settings where workspace overrides are permitted. |
| Octocode home (normally `~/.octocode/`) | Shared `.env` and `.octocoderc`, saved logins, installed skills, and caches. `OCTOCODE_HOME` selects another directory. |

Highest priority first: process environment → project `.env` → home `.env` → project `.octocoderc` → home `.octocoderc` → default. Run `npx -y octocode config list` to inspect effective values and sources, or `npx -y octocode config home` to locate the active home. Removing an override with `config unset` lets the next applicable source supply the value. [Settings and workspace permissions](https://github.com/bgauryy/octocode/blob/main/docs/CONFIGURATION.md)

## GitHub credential selection

For the configured host, Octocode checks process tokens, project `.env`, home `.env`, the encrypted Octocode login, the OS credential store, then the host’s GitHub CLI login. Within a single environment layer, `GH_TOKEN` wins over `GITHUB_TOKEN`. Public endpoints that allow anonymous access work without a login.

- **Device login** — Use `npx -y octocode auth login` and check the account with `npx -y octocode auth status --json`. To remove the saved login for that host, use `npx -y octocode auth logout`.
- **CI or personal token** — Pass `GH_TOKEN` or `GITHUB_TOKEN` as a process secret. To save a personal token in the home `.env`, have the user enter it through `npx -y octocode config set GH_TOKEN --stdin`. This file stores the token in plain text.
- **GitHub Enterprise** — Set the Enterprise API root with `GITHUB_API_URL` in the process or home settings. Device login and refresh also require the host’s `OCTOCODE_GITHUB_CLIENT_ID`. [Authentication guide](https://github.com/bgauryy/octocode/blob/main/docs/AUTHENTICATION.md)

If the CLI and agent use different accounts, compare their process environments, Octocode homes, and hostnames. An environment token takes precedence over a new device login.

## Choose a tool

| Task | Start here |
|---|---|
| Read local code | Find paths with `structureSearch` or `localSearch` and read with `localFetch`. Use `astSearch` for syntax and `lspSearch` for callers or definitions. |
| Read GitHub code or history | Locate the repository with `ghSearchRepo`, find files with `ghStructure` or `ghSearchCode`, then use `ghGetFileContent`. For history, use `ghSearchHistory` then `ghGetHistoryItem`. |
| Check a dependency | Take the version from the lockfile, look it up with `artifactSearch` and check known advisories with `securityCheck`. |
| Explore dependencies locally | `ghCloneRepo` creates a checkout. Use `graphBuild` then `graphQuery` for dependency paths, cycles and change impact within an available graph scope. |
| Filter results by meaning | `clasify` can use local or GitHub reads, images, public pages and supported search results. A fresh evaluation sends evidence to the configured provider and can incur charges. |

## Optional tools

| To enable | Command |
|---|---|
| Graphs in MCP | `npx -y octocode config set local.beta true` with local tools enabled |
| Cloning in MCP | `npx -y octocode config set tools.enableClone true` (requires Git and persistent storage) |
| Advisory checks in MCP | `npx -y octocode config set tools.enableSecurityCheck true` |
| Classification in CLI and MCP | `npx -y octocode config set OCTOCODE_CLASSIFICATION_API --stdin` |

## Add and configure keys

Enter secrets on stdin and restart MCP after changing its environment or settings. For classification, `npx -y octocode config check OCTOCODE_CLASSIFICATION_API` checks that a key is present. A provider call can verify access and may incur a charge. For a custom gateway, check `classification.apiHost` before sending evidence. [Classification setup](https://github.com/bgauryy/octocode/blob/main/docs/OCTOCODE_CLASIFY.md)

Skills can have their own optional providers. Use `npx -y octocode skill info <name>` to check the skill’s requirements before adding keys.
