# Authentication

This page owns every Octocode credential flow: GitHub tokens, OAuth login and refresh, the `gh` CLI fallback, GitHub Enterprise, the `clasify` provider key, and npm registry credentials. Settings and file precedence in general are in [CONFIGURATION.md](CONFIGURATION.md); how credentials are protected is in [SECURITY.md](SECURITY.md#github-credentials).

All credential discovery runs in the native Rust runtime, so the CLI and the MCP server resolve the same token the same way.

## Quick start

```bash
npx octocode auth login     # GitHub OAuth device flow; stores an encrypted token
npx octocode auth --json    # verify: source, username, hostname
```

Prefer not to log in? Set `GITHUB_TOKEN` (or any [token variable](#token-environment-variables)) in your shell, CI, or MCP client `env` block, or run `gh auth login`.

## GitHub token resolution order

For each GitHub request, Octocode takes the first credential found for the request's host:

| # | Source | `auth status` label |
|---|---|---|
| 1 | A token environment variable (process env → workspace `.octocode/.env` → home `.octocode/.env`) | `env` |
| 2 | Octocode OAuth login: encrypted `<OCTOCODE_HOME>/credentials.json` | `octocode-storage` |
| 3 | An older native login in the OS credential store (macOS Keychain, Linux Secret Service, Windows Credential Manager) | `platform` |
| 4 | `gh auth token --hostname <host>` | `gh-cli` |

- **Environment always wins over stored logins.** `auth login` warns when a token variable is set, because the new login is not used until you unset it.
- An environment token is attached only to the host of the configured `GITHUB_API_URL` (`api.github.com` maps to `github.com`). It is never sent to a different host.
- A stored token that expires within 5 minutes is refreshed before use (see [Refresh](#refresh)). If refresh fails, resolution falls through to `gh`.
- With no credential at all, public GitHub requests run unauthenticated at GitHub's lower rate limit (`publicGitHubAccess: "unauthenticated"` in `auth --json`).

Code: `packages/octocode-native/crates/runtime/src/providers/github/auth/resolver.rs`, `credential_store.rs`, `discovery.rs`.

### Token environment variables

| Priority | Variable | Typical origin |
|---|---|---|
| 1 | `OCTOCODE_TOKEN` | Octocode-specific override; the VS Code extension writes this one |
| 2 | `GH_TOKEN` | GitHub CLI convention |
| 3 | `GITHUB_TOKEN` | GitHub Actions |
| 4 | `GITHUB_PERSONAL_ACCESS_TOKEN` | Personal access token |

Source wins before alias: any nonblank process variable beats every `.env` file, and a workspace `.env` token beats a home `.env` token even when it uses a lower-priority name. Alias priority only breaks ties inside one source. `.octocoderc` never supplies GitHub tokens. The same list is generated from `packages/octocode-config/config-contract.json` into [CONFIG_SETTINGS.md](generated/CONFIG_SETTINGS.md#github-token-priority).

## OAuth device login

```bash
npx octocode auth login                 # device flow for the configured host
npx octocode auth login --force         # switch accounts; old login kept until the new one is saved
npx octocode auth login --json          # machine-readable result
```

1. Octocode requests a device code with scopes `repo read:org gist` and prints the verification URL and one-time code on stderr. Open the URL in a browser and enter the code.
2. It polls GitHub until you approve, then saves the token and your username.
3. If a valid stored login already exists for that host, `login` does nothing unless you pass `--force`.

Login needs an interactive terminal; in CI, set a token variable instead. On `github.com` Octocode uses its built-in OAuth app; other hosts need `OCTOCODE_GITHUB_CLIENT_ID` (see [GitHub Enterprise](#github-enterprise)).

### Where the token is stored

- `<OCTOCODE_HOME>/credentials.json` (default `~/.octocode/credentials.json`), one entry per host, encrypted with AES-256-GCM.
- The key is in `<OCTOCODE_HOME>/.key` next to it. Both files are `0600` on Unix, written atomically under a lock. Anyone who can read both files can decrypt the token.
- Logins made by older native versions in the OS credential store are still read (source `platform`) and refreshed in place, but new logins always go to `credentials.json`.

Protection details are in [SECURITY.md](SECURITY.md#github-credentials).

### Refresh

GitHub App user tokens expire; classic `ghp_` tokens and environment tokens are never refreshed.

- **Automatic:** a stored token within 5 minutes of expiry is refreshed on the next request, and the result is written back to the store it came from. A cross-process lock makes concurrent processes spend the single-use refresh token only once.
- **Manual:** `npx octocode auth login --refresh` exchanges the stored refresh token without a new device flow.
- An expired refresh token needs a new `auth login`.

### Status and logout

```bash
npx octocode auth              # same as `auth status`
npx octocode auth status --json
npx octocode auth logout
```

- `status` reports `authenticated`, `username`, `hostname`, `tokenSource` (`env`, `octocode-storage`, `platform`, `gh-cli`) and `publicGitHubAccess`. It never prints the token and does not refresh.
- `logout` removes the configured host's login from `credentials.json` and from the OS credential store. It never changes environment variables or your `gh` login.
- `status`, `login` and `logout` all target the host derived from `GITHUB_API_URL`; `login --hostname` overrides it for one login.

Code: `packages/octocode-native/crates/cli/src/cli/system.rs`, `crates/runtime/src/providers/github/login.rs`.

## gh CLI passthrough

After `gh auth login`, Octocode runs `gh auth token --hostname <host>` as the last source. The child process gets no Octocode token variables (`OCTOCODE_TOKEN`, `GH_TOKEN`, `GITHUB_TOKEN`, `GITHUB_PERSONAL_ACCESS_TOKEN`, `GH_ENTERPRISE_TOKEN`, `GITHUB_ENTERPRISE_TOKEN`), so `gh` answers from its own host-scoped login. Common Homebrew paths are added to `PATH`. The call is bounded to 5 seconds; a missing or failing `gh` is simply skipped.

## GitHub Enterprise

```bash
export GITHUB_API_URL="https://github.mycompany.com/api/v3"   # shell or home config only
export GITHUB_TOKEN="ghp_your_ghe_token"                       # or use device login:
export OCTOCODE_GITHUB_CLIENT_ID="your_oauth_app_client_id"    # OAuth app registered on that host
npx octocode auth login --hostname github.mycompany.com
```

- `GITHUB_API_URL` (or `github.apiUrl` in the home `.octocoderc`) selects the API root and the credential host. It is home-trusted: a workspace `.env` or `.octocoderc` cannot redirect tokens.
- Device login and refresh on any host other than `github.com` require `OCTOCODE_GITHUB_CLIENT_ID`; without it `login` fails with that message.
- `gh` passthrough asks for the enterprise host by name.

## MCP clients and the VS Code extension

- The MCP server resolves tokens with the same order. Pass a token in the client `env` block, or rely on `auth login` / `gh` on the same machine. See [OCTOCODE_MCP.md](OCTOCODE_MCP.md).
- The VS Code extension (`octocode-mcp-vscode`) signs in through the editor's GitHub account and writes the token as `OCTOCODE_TOKEN` into the MCP configs it manages (files written `0600`), removing the `GITHUB_TOKEN` key older versions wrote.
- Restart the MCP server after changing a token variable or `.env` file.

## Classification key (`clasify`)

`clasify` sends evidence to an external classification provider, so it only exists when a key is set.

| Variable | Purpose |
|---|---|
| `OCTOCODE_CLASSIFICATION_API` | Provider API key (bearer). Alias: `OCTOCODE_JEV_KEY` (the Jev vendor's native name; the canonical name wins within one source) |
| `OCTOCODE_CLASSIFICATION_API_HOST` | Optional API root override (default `https://api.typesafe.ai`). Must be HTTPS except on loopback. Home-trusted |
| `OCTOCODE_CLASSIFICATION_TYPE` | Vendor; only `jev` today |

- The key follows the same source order as GitHub tokens: process env → workspace `.env` → home `.env` → `.octocoderc` (`classification.api`). It never appears in resolved configuration output.
- **No key:** MCP does not register `clasify`, its instructions never mention it, and no tool returns a `next.clasify` step. The CLI still lists it in `octocode scheme` with `availability.enabled: false` and `envVar`, and a direct call fails with `missingConfiguration` (exit 5).
- **Kill switch:** a present-but-blank `OCTOCODE_CLASSIFICATION_API=` in the process environment disables classification for that process, even if a `.env` file has a key.
- Get a key from the [provider docs](https://docs.typesafe.ai/introduction). What is sent is in [SECURITY.md](SECURITY.md#classification-egress); usage in [OCTOCODE_CLASIFY.md](OCTOCODE_CLASIFY.md).

## npm registry credentials

`artifactSearch` with `type:"npm"`:

- queries the registry named in the query's `registry` field, or `https://registry.npmjs.org/` by default;
- reads credentials only from the user npmrc: `NPM_CONFIG_USERCONFIG` (or `npm_config_userconfig`), else `~/.npmrc`. A project `.npmrc` is never read, because a repository must not be able to steer your token;
- attaches a token only when its `//host[:port]/path/:_authToken` (Bearer) or `:_auth` (Basic) key matches the registry origin; the longest path prefix wins, and `${VAR}` references are expanded (an unset variable voids the entry);
- rejects credentials, query strings or fragments inside registry URLs, and blocks private, loopback and link-local registries unless `OCTOCODE_ALLOW_PRIVATE_REGISTRY=true` (home-trusted).

Code: `packages/octocode-native/crates/runtime/src/providers/artifact/npmrc.rs`, `crates/runtime/src/tools/artifact_search/mod.rs`.

## Troubleshooting

| Symptom | Fix |
|---|---|
| 401, or `publicGitHubAccess: "unauthenticated"` | `npx octocode auth login`, or set `GITHUB_TOKEN` in the shell or MCP `env` block |
| Wrong account | `auth login --force`, or `auth logout` then `auth login` |
| New login ignored | A token variable wins; unset it (`auth --json` shows `tokenSource: "env"`) |
| `login requires an interactive terminal` | Use a token variable in CI and scripts |
| Enterprise login or refresh rejected | Set `OCTOCODE_GITHUB_CLIENT_ID` for that host and `GITHUB_API_URL` in the shell or home config |
| Enterprise requests hit github.com | `GITHUB_API_URL` was set in a workspace file (ignored); set it in the shell or home config |
| `clasify` missing | Set `OCTOCODE_CLASSIFICATION_API`; check it is not blank in the process env |
| Private npm package not found | Put a registry-scoped `_authToken` in the user npmrc and pass that `registry` in the query |
