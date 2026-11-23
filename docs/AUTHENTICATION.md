# Authentication

## OAuth device login

From an interactive terminal, start GitHub login, open the URL Octocode prints, and enter the one-time code. Review the requested `repo`, `read:org` and `gist` scopes before approving. The CLI stores the login for that host. The MCP server can use it from the same Octocode home.

```bash
npx -y octocode auth login
npx -y octocode auth status --json
```

Check for `verification: "verified"` and the expected account and host. A saved device login reports `tokenSource: "octocode-storage"` when selected. If you already use GitHub CLI, `gh auth login` also works. To save a personal token locally, use `npx -y octocode config set GH_TOKEN --stdin`. In CI, pass a secret as `GH_TOKEN` or `GITHUB_TOKEN` to the process.

## GitHub token resolution

Octocode selects credentials for the host named by `GITHUB_API_URL` or `github.apiUrl` (default `github.com`). Run `octocode auth status --json` to see the `hostname`, `username`, `tokenSource` and verification result.

| Priority | `tokenSource` | Where Octocode looks |
|---:|---|---|
| 1 | `env` | Process environment, then workspace `.octocode/.env`, then user `.env`. The first layer with a nonblank token wins. Within that layer: `GH_TOKEN`, legacy `OCTOCODE_TOKEN`, `GITHUB_TOKEN`, legacy `GITHUB_PERSONAL_ACCESS_TOKEN`. |
| 2 | `octocode-storage` | The host’s encrypted login in the Octocode home, saved by `octocode auth login`. |
| 3 | `platform` | An existing Octocode entry for the host in macOS Keychain, Windows Credential Manager or Linux Secret Service. |
| 4 | `gh-cli` | The saved login returned by `gh auth token --hostname <host>`. Octocode clears token environment variables for this lookup. |
| 5 | `none` | Anonymous access where GitHub permits it. Code search and private repositories require authentication. |

An explicit environment token that GitHub rejects stops the request. Replace it at its source to switch accounts. For a saved login that is due for refresh, a tool request tries to refresh it and can fall back to the host’s `gh` login. Inspecting status does not refresh the login. [Configuration file locations](CONFIGURATION.md#where-every-value-comes-from)

### Status and logout

| Goal | Command | What to expect |
|---|---|---|
| Verify the active credential | `npx -y octocode auth status --json` | Reports host, account, source and verification. Available quota buckets include remaining requests and reset times. |
| Switch stored accounts | `npx -y octocode auth login --force` | Replaces the saved login after successful approval. An environment token still takes priority. |
| Remove the host's saved login | `npx -y octocode auth logout` | Clears the Octocode and OS entries for the configured host. Manage environment tokens and `gh` logins at their source. |

## GitHub Enterprise

Set the enterprise API root in the process or home configuration. For device login or refresh, supply the client ID of an OAuth app registered on that host. The built-in client is for `github.com`.

```bash
export GITHUB_API_URL="https://github.example.com/api/v3"
export OCTOCODE_GITHUB_CLIENT_ID="your-enterprise-oauth-client-id"
npx -y octocode auth login
npx -y octocode auth status --json
```

For token-based access, set the same API root and supply `GH_TOKEN` or `GITHUB_TOKEN` for that host. Octocode ignores `GH_ENTERPRISE_TOKEN` and `GITHUB_ENTERPRISE_TOKEN`, including during the `gh` fallback. For a saved GitHub CLI account, use `gh auth login --hostname <host>`. `auth login --hostname <host>` overrides the hostname for one login. Set `GITHUB_API_URL` or home `github.apiUrl` so later requests, status, and logout target that host.

## MCP clients and the VS Code extension

Put a token in the MCP server's process environment or let it use the host's Octocode or `gh` login. Restart MCP after changing its environment or `.env`. The VS Code extension signs in through the editor and writes `GH_TOKEN` to the MCP configurations it manages. [MCP setup](OCTOCODE_MCP.md)

## Classification key (`clasify`)

Classification is off by default. Set a Jev key in your Octocode home to enable `clasify` in the CLI and MCP. Supply the key on standard input and restart MCP. [Get a key](https://docs.typesafe.ai/introduction)

```bash
npx -y octocode config set OCTOCODE_CLASSIFICATION_API --stdin
npx -y octocode config check OCTOCODE_CLASSIFICATION_API
```

A nonblank key enables discovery. A provider call verifies access. The key can also come from the process environment or home `classification.api`. A blank process variable explicitly disables classification. [Classification guide](OCTOCODE_CLASIFY.md) · [Change the provider host](CONFIGURATION.md#change-the-classification-host)

## npm registry credentials

For `artifactSearch`, select `ecosystem: "npm"` and set `registryUrl` for a custom registry. The default is `https://registry.npmjs.org/`. Put the registry's credential in your **user npmrc**, selected by `NPM_CONFIG_USERCONFIG`, then `npm_config_userconfig`, then `~/.npmrc`.

```ini
//registry.example.com/npm/:_authToken=${NPM_TOKEN}
```

Set `NPM_TOKEN` in the environment for this example. The npmrc entry's host and path must match your registry. A private or loopback registry also needs `network.allowPrivateRegistry: true` in trusted configuration. [Configuration](CONFIGURATION.md#all-settings-and-defaults)

## Troubleshooting

| Symptom | Next step |
|---|---|
| A new login appears to have no effect | If `tokenSource` is `env`, replace or remove the winning variable. Otherwise check the host and the Octocode home of the process making the call. |
| 401 or `verification: "invalid"` | Replace an environment token at its source, renew a saved login with `auth login --force`, or sign in again through `gh`, according to `tokenSource`. |
| Device login requires a terminal | Run the login interactively or pass a token through the CI secret store. |
| An enterprise request goes to github.com | Set `GITHUB_API_URL` in the process or home configuration. Workspace files cannot select the credential host. |
| A private npm lookup fails | Check the query's registry URL against the user npmrc scope and confirm any referenced token variable is set. |
