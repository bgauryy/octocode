# Octocode MCP server

The Octocode MCP server is the toolkit's standard interface for AI coding clients. It exposes Octocode's research tools through the Model Context Protocol over stdio. The server is intentionally thin: it registers schemas and transports requests, while tool behavior and distribution live in `@octocodeai/octocode-native`; reusable primitives remain isolated in its engine crate and `./engine` subpath.

Use this page for the MCP mental model, startup lifecycle, client configuration entry points, and session persistence. For every tool, see [Octocode tools reference](OCTOCODE_TOOLS.md). For settings, see [Octocode configuration](CONFIGURATION.md); for GitHub tokens, login, and credential storage, see [Authentication](AUTHENTICATION.md).

## What MCP adds

MCP gives assistants a stable tool catalog instead of making them shell out by hand. In Octocode, MCP and CLI share the same schemas, runners, security validation, response envelope, pagination, and secret redaction path. A query researched through an assistant and a query run through `npx octocode <toolName> '<json>'` exercise the same core implementation.

| Layer | Responsibility |
|-------|----------------|
| MCP server | stdio lifecycle, tool registration, client-facing descriptions, output sanitization boundary |
| Tools core | GitHub/package/local/LSP runners, credentials, config, session, pagination, response shaping |
| Engine | native ripgrep, structural AST search, minify/signatures, secret scan, LSP orchestration |

A request flows through catalog registration → strict schema validation → security/config gates → native runtime runner → provider, filesystem, graph, or language-server boundary → sanitized structured/text response. The outer batch envelope, the optional `mainGoal`/`reasoning` brief, result indexes, partial failures, `next` pages, and `hints` leads are documented once in [How every tool call works](OCTOCODE_TOOLS.md#how-every-tool-call-works). MCP does not maintain a second copy of those contracts.

## Quick start

Install through the CLI helper when you can:

```bash
npx octocode install --ide cursor
```

Otherwise, configure an MCP client directly to run `octocode-mcp`:

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

Set a token in the client `env` block (`GITHUB_TOKEN`, `GH_TOKEN`, or `OCTOCODE_TOKEN`), or run `npx octocode auth login` once on the same machine; the server also falls back to `gh auth token`. `.octocoderc` never supplies tokens. See [Authentication](AUTHENTICATION.md).

## Startup lifecycle

The MCP entrypoint runs these steps in order:

```text
loadNativeBinding
  -> new NativeRuntime({ surface: 'mcp' })
  -> ABI version check
  -> catalog()                    (tool availability from native)
  -> contract fingerprint check   (core ↔ native schema parity)
  -> registerTool loop            (Zod schemas via @octocodeai/config/schema)
  -> StdioServerTransport connect
```

At startup, the Node adapter loads the platform-specific Rust N-API addon (`@octocodeai/octocode-native`), instantiates the native runtime, and validates that its ABI version and contract fingerprint match the registered schemas (`@octocodeai/config/schema`, which re-exports `@octocodeai/octocode-core`). A mismatch on either check is a startup failure, so the server never serves a schema the runtime would reject. The only escape hatch is `OCTOCODE_ALLOW_CONTRACT_DRIFT=1`, which downgrades a fingerprint mismatch to a stderr warning; it is ignored under `NODE_ENV=production`, and the bundled server honors it only with `NODE_ENV=development` or `test`. Startup also fails when no tool is available (for example, a `TOOLS_TO_RUN` list with only unknown names).

Tool arguments pass through the runtime's `normalizeInput` before SDK validation, the same native normalization the CLI applies: a bare query is wrapped in `queries`, and a JSON-encoded or bare-scalar value for a list-only field, or an exact integer/boolean string for an integer/boolean-only field, is repaired. Fields that also accept a string are never rewritten.

Configuration, security policy, providers, credentials, caches, and usage stats are owned entirely by the native runtime; the Node adapter owns only protocol framing and process lifecycle. Settings and environment tokens are resolved once at startup. Stored logins and `gh` are resolved per request, so a new `octocode auth login` takes effect without a server restart.

## Tool catalog

The full discovery catalog contains 16 tools. With default settings and no
provider key, the MCP server registers 12: `ghCloneRepo` and `astRewrite` are
CLI-only and always omitted. `clasify` needs a nonblank resolved classification key: `OCTOCODE_CLASSIFICATION_API`, else the selected vendor's key (`OCTOCODE_JEV_KEY` for jev), else `.octocoderc` `classification.api` (a present-but-blank `OCTOCODE_CLASSIFICATION_API` disables it);
`astTopology` needs `OCTOCODE_BETA=true` (or `local.beta:true`). Unavailable tools
are omitted from MCP discovery entirely, not registered as failing calls. The
CLI-only exclusion is enforced twice: the native runtime never lists them for
the MCP surface, and the adapter filters core's `isCliOnlyTool` policy again
before registration.

| Family | Tools |
|--------|-------|
| GitHub | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` (CLI-only) |
| Local | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, `astTopology`, `lspSearch` (`astRewrite` is CLI-only) |
| Package | `artifactSearch` |
| Semantic assessment | `clasify` |

`astRewrite` is never registered on MCP; run `octocode astRewrite` with the
same beta gate. It is preview-first; applying a mutation requires the complete
set of preview hashes.

Server instructions are built for the registered tool subset and target at most
2,000 characters, because hosts truncate near 2 KB. The budget is the core
constant `MAX_MCP_INSTRUCTION_CHARS`; it is enforced by tests
(`packages/octocode-mcp/tests/native/create-native-mcp.test.ts`), not by runtime
truncation. The runtime grammar inventory and `scheme` guidance are CLI-only;
MCP clients get schemas through `tools/list`.

To read the live CLI catalog, run `octocode scheme`.

GitHub discovery is three tools with no `operation` field: `ghSearchRepo`
(repositories), `ghSearchCode` (indexed code), and `ghStructure` (repository
tree). Removed compatibility names cannot be re-enabled.

Every tool accepts bulk input through `queries`, with up to 5 items per call. MCP
publishes executable input schemas, titles, and descriptions for the registered
tools. It does not publish output schemas; core and the native runtime retain them for
internal result validation and drift detection. Runtime results use the shared
structured bulk envelope with per-query success, empty, and error states, plus
typed evidence and pagination data when more content is available. For the
complete response and continuation rules, see the [Octocode tools reference](OCTOCODE_TOOLS.md).

## Configuration and auth

Set per-client or per-project settings in the MCP client's `env` block. File
settings come from `<octocode-home>/.octocoderc` and the workspace
`<cwd>/.octocode/.octocoderc`; environment variables win over file values. The
settings that most often differ per MCP client:

| Setting | Default | Why it matters |
|---------|---------|----------------|
| `GITHUB_TOKEN` / `GH_TOKEN` / `OCTOCODE_TOKEN` | — | GitHub API auth. See [Authentication](AUTHENTICATION.md). |
| `GITHUB_API_URL` | `https://api.github.com` | GitHub Enterprise endpoint. |
| `ENABLE_LOCAL` | `true` | Turns local filesystem and LSP tools on or off. |
| `TOOLS_TO_RUN` / `DISABLE_TOOLS` | unset | Strict allowlist (replaces the default set) / removals from the default set. |
| `WORKSPACE_ROOT`, `ALLOWED_PATHS` | — | Bound local path resolution and validation. |
| `OCTOCODE_BETA` | `false` | Registers `astTopology`. |
| `OCTOCODE_CLASSIFICATION_API` (or `OCTOCODE_JEV_KEY`) | unset | Registers `clasify`. See [Authentication](AUTHENTICATION.md#classification-key-clasify). |
| `OCTOCODE_OUTPUT_FORMAT` | `yaml` | Encoding of the MCP text channel (`yaml` or `json`); `structuredContent` is always JSON. |

Every other setting (timeouts, retries, storage mode, pagination budget, LSP config, classification host) is in the [configuration reference](CONFIGURATION.md#all-settings-reference). Development-only overrides (`OCTOCODE_NATIVE_BINDING`, `OCTOCODE_ALLOW_CONTRACT_DRIFT`) are listed in [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md#development-environment-variables).

## Tool name migration

Tool names were renamed in v18 to use camelCase. If you have `TOOLS_TO_RUN` or `DISABLE_TOOLS` set with old names, update them. **Old names are not recognized and are silently ignored**; when no name in `TOOLS_TO_RUN` is valid, startup fails with `No native tools are available`. Use the table below to find the new name.

This migration table is historical. The names in the Old name column are not
active catalog entries.

| Old name | New name |
|---|---|
| `github_search_code` | `ghSearchCode` |
| `github_fetch_content` | `ghGetFileContent` |
| `github_view_repo_structure` | `ghStructure` |
| `github_search_repos` | `ghSearchRepo` |
| `github_search_pull_requests` | `ghSearchHistory` |
| `github_clone_repo` | `ghCloneRepo` |
| `local_analyze_graph` | `astTopology` |
| `local_fetch_content` | `localFetch` |
| `local_dead_code` | `astTopology` (`analysis:"deadCode"`) |
| `local_find_files` | `structureSearch` (`files` operation) |
| `local_ripgrep` | `localSearch` (lexical `searchText`) |
| `local_view_structure` | `structureSearch` (`tree` operation) |
| `local_search` | `localSearch` ✅ unchanged |
| `lsp` | `lspSearch` |
| `package_search` | `artifactSearch` |

## Materialization and response cache

The MCP server shares the same on-disk cache as the CLI under the configured Octocode home:

| Bucket | Path | Contents |
|---|---|---|
| Clone | `tmp/clone/{owner}/{repo}/{branch}` | Reusable Git checkouts |
| Tree | `tmp/tree/{owner}/{repo}/{commitSha}` | Materialized repository trees |
| Response | `tmp/response/` | Eligible GitHub and npm response payloads |

Each runtime start (MCP or CLI) runs a best-effort sweep when the 24-hour marker `tmp/.last-cache-maintenance` is due. It removes expired entries from Octocode's own buckets, leaves unrelated files under `tmp` alone, is skipped in `memory` storage mode, and a sweep failure never fails startup. See [Cache storage and lifecycle](CONFIGURATION.md#cache-storage-and-lifecycle) for the 24-hour gate, expiry rules, limits, and manual controls, and [Cache behavior](OCTOCODE_TOOLS.md#cache-behavior) for tool-level semantics.

## Usage stats

The native runtime can record classification usage in `<octocode-home>/stats.json`
(`stats.clasify.calls`, `input_tokens`, `output_tokens`). It is off by default:
set `OCTOCODE_ENABLE_STATS=true` with persistent storage. Updates are
best-effort, serialized with a `stats.json.lock` sidecar, and written through a
temp file and atomic rename, so concurrent MCP and CLI processes can share one
home. A stats failure never fails a tool call. Octocode does not write a
separate session file.

## See also

- [Octocode tools reference](OCTOCODE_TOOLS.md)
- [Octocode configuration](CONFIGURATION.md)
- [Authentication](AUTHENTICATION.md)
- [Octocode CLI guide](../packages/octocode/docs/OCTOCODE_CLI.md)
- [Security](SECURITY.md)
