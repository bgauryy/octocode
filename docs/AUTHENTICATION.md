# Authentication

This page owns every Octocode credential flow: GitHub tokens, OAuth login and refresh, the `gh` fallback, GitHub Enterprise, the `clasify` key, and npm registry credentials. The native runtime does all credential discovery, so the CLI and the MCP server resolve the same token. File precedence: [CONFIGURATION.md](CONFIGURATION.md). Credential protection: [SECURITY.md](SECURITY.md#github-credentials).

```bash
npx octocode auth login            # GitHub OAuth device flow; stores an encrypted token
npx octocode auth status --json    # verify: source, username, hostname
```

To skip login, set `GITHUB_TOKEN` (or `GH_TOKEN`) in the shell, CI, or MCP client `env` block, or run `gh auth login`.

## GitHub token resolution

Which credential a request uses, in order, is in [GitHub token resolution](CONFIGURATION.md#github-token-resolution).

## OAuth device login

```bash
npx octocode auth login           # device flow for the configured host
npx octocode auth login --force   # switch accounts; old login kept until the new one is saved
npx octocode auth login --json    # machine-readable result
```

1. Octocode requests a device code with scopes `repo read:org gist` and prints the URL and one-time code on stderr. Open the URL and enter the code.
2. It polls GitHub until you approve, then saves the token and your username.
3. If a valid stored login exists for that host, `login` does nothing without `--force`.

Login needs an interactive terminal; in CI, set a token variable. `github.com` uses Octocode's built-in OAuth app; other hosts need `OCTOCODE_GITHUB_CLIENT_ID`.

**Storage:** `<OCTOCODE_HOME>/credentials.json` (default `~/.octocode/credentials.json`) holds one AES-256-GCM entry per host. The key is in `.key` beside it. Both are `0600` on Unix and written atomically under a lock; anyone who can read both files can decrypt the token. Logins in the OS credential store from older native versions are still read (source `platform`) and refreshed in place; new logins always go to `credentials.json`.

### Refresh

GitHub App user tokens expire; classic `ghp_` tokens and env tokens are never refreshed.

- **Automatic:** a stored token within 5 minutes of expiry is refreshed on the next request and written back to its store. A cross-process lock spends the single-use refresh token only once.
- **Manual:** `npx octocode auth login --refresh` exchanges the stored refresh token without a device flow.
- An expired refresh token needs a new `auth login`.

### Status and logout

```bash
npx octocode auth status
npx octocode auth status --json
npx octocode auth logout
```

- `status` reports `authenticated`, `username`, `hostname`, `tokenSource` (`env`, `octocode-storage`, `platform`, `gh-cli`, or `none`), and `verification` (`verified`, `unverified`, `invalid`, `none`). It never prints the token and does not refresh.
- `logout` removes the host's login from `credentials.json` and the OS credential store. It never changes env vars or your `gh` login.
- All three commands target the host from `GITHUB_API_URL`; `login --hostname` overrides it for one login.

## gh CLI passthrough

As the last source, Octocode runs `gh auth token --hostname <host>` without the token variables (`GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN`, `GITHUB_ENTERPRISE_TOKEN`), so `gh` answers from its own host-scoped login. Common Homebrew paths are added to `PATH`. The call is bounded to 5 seconds; a missing or failing `gh` is skipped.

## GitHub Enterprise

```bash
export GITHUB_API_URL="https://github.mycompany.com/api/v3"   # shell or home config only
export GITHUB_TOKEN="ghp_your_ghe_token"                       # or use device login:
export OCTOCODE_GITHUB_CLIENT_ID="your_oauth_app_client_id"    # OAuth app registered on that host
npx octocode auth login --hostname github.mycompany.com
```

- `GITHUB_API_URL` (or `github.apiUrl` in the home `.octocoderc`) selects the API root and the credential host. A workspace `.env` or `.octocoderc` cannot set it, so it cannot redirect tokens.
- Device login and refresh on any host other than `github.com` need `OCTOCODE_GITHUB_CLIENT_ID`; without it, `login` fails and names it.
- `gh` passthrough asks for the enterprise host by name.

## MCP clients and the VS Code extension

- The MCP server uses the same order. Put a token in the client `env` block, or rely on `auth login` or `gh` on the same machine.
- The VS Code extension (`octocode-mcp-vscode`) signs in through the editor's GitHub account and writes the token as `GH_TOKEN` into the MCP configs it manages (`0600`).
- Restart the MCP server after changing a token variable or `.env` file. A new `auth login` applies within a minute, without a restart.

## Classification key (`clasify`)

The classification key authorizes inline `classification` and `clasify` requests. [Configuration](CONFIGURATION.md#tool-availability-cli-and-mcp) owns CLI/MCP exposure and setup; provider readiness is checked when a request runs. Get a key from the [provider docs](https://docs.typesafe.ai/introduction); what is sent: [SECURITY.md](SECURITY.md#classification-egress).

| Variable | Purpose |
|---|---|
| `OCTOCODE_CLASSIFICATION_API` | Provider API key (bearer) |
| `OCTOCODE_CLASSIFICATION_API_HOST` | Optional API root (default `https://api.typesafe.ai`), `classification.apiHost` in `.octocoderc`. HTTPS except on loopback. Home-only. [Change it](CONFIGURATION.md#change-the-classification-host) |

- The key follows [configuration precedence](CONFIGURATION.md#where-every-value-comes-from), with `classification.api` as its settings-file fallback. It never appears in resolved configuration output.
- A present-but-blank `OCTOCODE_CLASSIFICATION_API=` in the process environment disables classification requests, even when a file has a key. It hides `clasify` from MCP; the CLI command stays listed and reports the missing credential when invoked.

## npm registry credentials

`artifactSearch` with `ecosystem:"npm"`:

- queries the query's `registryUrl`, or `https://registry.npmjs.org/` by default;
- reads credentials only from your user npmrc: `NPM_CONFIG_USERCONFIG` (or `npm_config_userconfig`), else `~/.npmrc`. A project `.npmrc` is never read, so a repository cannot steer your token;
- attaches a token only when its `//host[:port]/path/:_authToken` (Bearer) or `:_auth` (Basic) key matches the registry origin. The longest path prefix wins; `${VAR}` references expand, and an unset variable voids the entry. npmrc keys carry no scheme, so a token is never sent to a plain `http://` registry unless it is loopback (`localhost`, `127.0.0.1`, `::1`);
- rejects credentials, query strings, or fragments in registry URLs, and blocks private, loopback, and link-local registries unless `OCTOCODE_ALLOW_PRIVATE_REGISTRY=true` (home-only).

## Troubleshooting

| Symptom | Fix |
|---|---|
| 401, or `authenticated: false` | `npx octocode auth login`, or set `GITHUB_TOKEN` in the shell or MCP `env` block |
| Wrong account | `auth login --force`, or `auth logout` then `auth login` |
| New login ignored | A token variable wins; unset it (`auth status --json` shows `tokenSource: "env"`) |
| `login requires an interactive terminal` | Use a token variable in CI and scripts |
| Enterprise login or refresh rejected | Set `OCTOCODE_GITHUB_CLIENT_ID` for that host and `GITHUB_API_URL` in the shell or home config |
| Enterprise requests hit github.com | `GITHUB_API_URL` was set in a workspace file (ignored); set it in the shell or home config |
| `clasify` missing from MCP | Set a nonblank `OCTOCODE_CLASSIFICATION_API`, check the [MCP availability controls](CONFIGURATION.md#tool-availability-cli-and-mcp), and restart the server. |
| Classification call fails | Check the credential and returned provider error; CLI visibility and MCP key configuration do not prove provider readiness. |
| Private npm package not found | Put a registry-scoped `_authToken` in your user npmrc and pass that `registryUrl` in the query |
