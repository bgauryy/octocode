# Octocode MCP server

The Octocode MCP server exposes Octocode's research tools to AI coding clients over stdio. It is a thin adapter: it registers schemas and frames the protocol, while `@octocodeai/octocode-native` owns tool behavior, configuration, security policy, providers, credentials, caches, and stats. MCP and `npx octocode <toolName> '<json>'` share the same schemas, runners, validation, response envelope, pagination, and secret redaction.

Tool fields and the shared call contract (batch envelope, `mainGoal`/`reasoning`, partial failures, `next` pages, `hints` leads): [How every tool call works](OCTOCODE_TOOLS.md#how-every-tool-call-works). Settings: [CONFIGURATION.md](CONFIGURATION.md). Tokens: [AUTHENTICATION.md](AUTHENTICATION.md).

## Quick start

```bash
npx octocode install --ide cursor
```

Or configure the client directly:

```json
{
  "mcpServers": {
    "octocode": {
      "command": "npx",
      "args": ["-y", "octocode-mcp@latest"]
    }
  }
}
```

Set `GH_TOKEN` or `GITHUB_TOKEN` in the client `env` block, or run `npx octocode auth login` once on the same machine; the server also falls back to `gh auth token`.

## Startup

```text
loadNativeBinding → new NativeRuntime({ surface: 'mcp' }) → ABI version check
  → catalog() → contract fingerprint check → registerTool loop → StdioServerTransport connect
```

- The server loads the platform N-API addon and checks that its ABI version and contract fingerprint match the registered schemas. Either mismatch fails startup, so the server never serves a schema the runtime would reject.
- `OCTOCODE_ALLOW_CONTRACT_DRIFT=1` turns a fingerprint mismatch into a stderr warning. The bundled server honors it only with `NODE_ENV=development` or `test`, never under `production`.
- Startup fails with `No native tools are available` when no tool is available, for example a `TOOLS_TO_RUN` list of only unknown names.
- Settings and env tokens resolve once at startup. Stored logins and `gh` resolve per request, so a new `auth login` applies without a restart.

Native validates arguments once, as on the CLI, and an invalid call gets the same repair guidance (nearest field, allowed values, the missing field's fix). Every call is `{queries:[...]}`; a flat row or bare array is rejected. A JSON-encoded or bare-scalar value for a list-only field, or an exact integer or boolean string for an integer- or boolean-only field, is repaired; fields that also accept a string are never rewritten. In a batch, valid rows run and each invalid row returns its own error.

## Tool catalog

The discovery catalog has 16 tools. With default settings and no provider key, MCP registers 12. Unavailable tools are left out of `tools/list`, not registered as failing calls.

| Family | Tools |
|--------|-------|
| GitHub | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` (CLI-only) |
| Local | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, `lspSearch`, `astTopology` and `astRewrite` (both CLI-only) |
| Package | `artifactSearch` |
| Semantic assessment | `clasify` |

- `ghCloneRepo`, `astTopology`, and `astRewrite` are never registered on MCP, with or without `OCTOCODE_BETA`. No MCP description or lead names the beta tools; the shared prompt names them only in its `<cli>` section. Run them as `npx octocode astTopology` / `npx octocode astRewrite` with `OCTOCODE_BETA=true`; `astRewrite` applies only with the complete set of preview hashes.
- `clasify` needs a nonblank classification key and a passing startup provider check; otherwise it is left out. See [Clasify availability](OCTOCODE_CLASIFY.md#availability).
- GitHub discovery is three tools with no `operation` field: `ghSearchRepo` (repositories), `ghSearchCode` (indexed code), `ghStructure` (repository tree). Removed compatibility names cannot be re-enabled.
- Every tool takes up to 5 `queries` per call. MCP publishes input schemas, titles, and descriptions, not output schemas.
- Server instructions are one prompt for every surface and tool subset, at most 2,000 characters (`MAX_MCP_INSTRUCTION_CHARS`), because hosts truncate near 2 KB. The grammar inventory and `schema` guidance are CLI-only; MCP clients get schemas through `tools/list`. `npx octocode schema` prints the live CLI catalog.

## Configuration

Set per-client settings in the client `env` block; env vars beat `.octocoderc` files. The settings that most often differ per client:

| Setting | Default | Effect |
|---------|---------|--------|
| `GH_TOKEN` / `GITHUB_TOKEN` | — | GitHub API auth |
| `GITHUB_API_URL` | `https://api.github.com` | GitHub Enterprise endpoint |
| `OCTOCODE_ENABLE_LOCAL` | `true` | Local filesystem and LSP tools on or off |
| `TOOLS_TO_RUN` / `DISABLE_TOOLS` | unset | Strict allowlist / removals from the default set |
| `WORKSPACE_ROOT`, `ALLOWED_PATHS` | — | Bound local path resolution |
| `OCTOCODE_BETA` | `false` | Enables CLI-only beta tools; registers nothing on MCP |
| `OCTOCODE_CLASSIFICATION_API` | unset | Registers `clasify` ([key](AUTHENTICATION.md#classification-key-clasify)) |
| `OCTOCODE_OUTPUT_FORMAT` | `yaml` | Encoding of the MCP text channel (`yaml` or `json`); `structuredContent` is always JSON |

Every other setting: [generated settings](generated/CONFIG_SETTINGS.md). Development overrides (`OCTOCODE_NATIVE_BINDING`, `OCTOCODE_ALLOW_CONTRACT_DRIFT`): [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md#development-environment-variables).

The server shares the CLI's cache under the Octocode home and runs the same best-effort 24-hour sweep at start: [Cache storage and lifecycle](CONFIGURATION.md#cache-storage-and-lifecycle), and [Cache behavior](OCTOCODE_TOOLS.md#cache-behavior) for tool-level rules. Opt-in usage stats: [Clasify usage stats](OCTOCODE_CLASIFY.md#usage-stats). Octocode writes no session file.

## Tool names before v18

Names before v18 are not recognized and are ignored without a warning; if no `TOOLS_TO_RUN` name is valid, startup fails. Update `TOOLS_TO_RUN` and `DISABLE_TOOLS`:

| Old name | New name |
|---|---|
| `github_search_code` | `ghSearchCode` |
| `github_fetch_content` | `ghGetFileContent` |
| `github_view_repo_structure` | `ghStructure` |
| `github_search_repos` | `ghSearchRepo` |
| `github_search_pull_requests` | `ghSearchHistory` |
| `github_clone_repo` | `ghCloneRepo` |
| `local_analyze_graph` | `astTopology` (CLI-only) |
| `local_dead_code` | `astTopology` (CLI-only, `operation:"deadCode"`) |
| `local_fetch_content` | `localFetch` |
| `local_find_files` | `structureSearch` (`files`) |
| `local_view_structure` | `structureSearch` (`tree`) |
| `local_ripgrep` | `localSearch` (lexical `matchString`) |
| `local_search` | `localSearch` (unchanged) |
| `lsp` | `lspSearch` |
| `package_search` | `artifactSearch` |

## See also

- [Octocode tools reference](OCTOCODE_TOOLS.md)
- [Octocode CLI guide](../packages/octocode/docs/OCTOCODE_CLI.md)
- [Security](SECURITY.md)
