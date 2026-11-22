# Authentication

## OAuth device login

From an interactive terminal, start GitHub login, open the URL Octocode prints, and enter the one-time code. Review the requested `repo`, `read:org` and `gist` scopes before approving. The CLI stores the login for that host. The MCP server can use it from the same Octocode home.

```bash
npx -y octocode auth login
npx -y octocode auth status --json
```

Look for `verification: "verified"`, the expected `username` and `hostname`, and `tokenSource: "octocode-storage"`. If you already use GitHub CLI, `gh auth login` also works. To save a personal token locally, use `npx -y octocode config set GH_TOKEN --stdin`. In CI, pass a secret as `GH_TOKEN` or `GITHUB_TOKEN` to the process.

## GitHub token resolution

If the wrong account appears, check `tokenSource` and `hostname` in `octocode auth status --json`.

| Source | Credential |
|---|---|
| `env` | First matching token in the process, workspace `.octocode/.env`, or home `.env`, in that order. At the same source, `GH_TOKEN` wins over `GITHUB_TOKEN`. |
| `octocode-storage` | The login saved by `octocode auth login` for this host. |
| `platform` | An existing login in the OS credential store. |
| `gh-cli` | Your GitHub CLI login for this host. |
| `none` | Public endpoints may allow anonymous access. Private repositories and code search require sign-in. |

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

For token-based access, set the same API root and supply a token for that host. `auth login --hostname <host>` overrides the hostname for one login. Set `GITHUB_API_URL` or home `github.apiUrl` so later requests, status, and logout target that host.

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
