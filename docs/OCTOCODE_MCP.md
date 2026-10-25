# Octocode MCP server

The Octocode MCP server exposes Octocode's research tools to AI coding clients over stdio. The server is the native binary's `octocode mcp` command; `octocode-mcp` is a thin Node launcher that starts it. The binary owns protocol framing, tool listing, tool behavior, configuration, security policy, providers, credentials, caches, and stats. MCP and `npx octocode <toolName> '<json>'` share the same schemas, runners, validation, response envelope, pagination, and secret redaction.

Tool fields and the shared call contract (batch envelopes, indexed results, `next` continuations, `related` calls, and prose `hints`): [How every tool call works](OCTOCODE_TOOLS.md#how-every-tool-call-works). Settings: [CONFIGURATION.md](CONFIGURATION.md). Tokens: [AUTHENTICATION.md](AUTHENTICATION.md).

`npx octocode-mcp` needs Node.js (it starts the platform binary and forwards stdio). The standalone `octocode` binary serves MCP without Node: `octocode mcp`. Public Agent Skills are installed through the CLI’s embedded bundle, not an MCP tool. See [CLI interfaces](OCTOCODE_CLI.md) and [skill installation](OCTOCODE_CLI.md#skill--agent-skills).

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
octocode-mcp (Node launcher) → platform `octocode mcp` (same stdio, same exit code)
  → ToolRuntime::from_host(surface: mcp) → catalog → listing from the embedded presentation → rmcp over stdio
```

- One process serves both protocol eras: a 2025-era client opens with `initialize`, a 2026-07-28 client with `server/discover` (or a versioned request). The tool list, descriptions, and instructions come from the contract embedded in the binary, so the server never serves a schema the runtime would reject; there is no separate addon, ABI, or fingerprint to match.
- A call to a name `tools/list` did not return is a JSON-RPC `-32602` (`Tool X not found`).
- Startup fails with `No native tools are available` when no tool is available, for example a `TOOLS_TO_RUN` list of only unknown names. The launcher (and the binary) answer the client's first request with that reason as a JSON-RPC `-32603` error, then exit 1, so the client shows why. The launcher does the same when the platform binary is missing.
- Settings and env tokens resolve once at startup. Stored logins and `gh` are resolved by native credential discovery; see [credential refresh and caching](AUTHENTICATION.md).
- Shutdown: stdin EOF, SIGINT or SIGTERM cancel in-flight calls and close the runtime. After EOF the server still writes responses for calls already running, up to 5 seconds, so a client that closes stdin right after its last request still gets the answer. A client that sends a line longer than 16 MiB loses that line.
- A long call does not block the others: pings, listing and short reads answer while a heavy call runs, and a cancellation reaches the call at once.

Native validates the same JSON input on CLI and MCP: `{ "queries": [...] }` with optional top-level `responseChars`, or `{ "cursor": "…" }` copied from an issued continuation. Types, field names, operation variants, and bounds are exact. Invalid input returns field paths and schema guidance; it is not rewritten into a guessed request. In a batch, valid rows run and each invalid row returns its own indexed error.

Results use `results: [{index, data, status?, rowPart?}]`. Required work appears under `next` as `{kind:"continue", tool, input:{cursor}}`; optional exploration appears under `related` as `{kind:"related", tool, input:{queries:[...]}}`. Copy the call's `input` unchanged to its named tool. `hints` contain explanatory text.

## Tool catalog

The contract has 14 tools. With default settings MCP registers 12; a classification credential and successful provider ping add `clasify` (13). Unavailable tools are left out of `tools/list`, not registered as failing calls.

| Family | Tools |
|--------|-------|
| GitHub | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` (CLI-only) |
| Local | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, `lspSearch` |
| Package | `artifactSearch` |
| Semantic assessment | `clasify` |

- `ghCloneRepo` is never registered on MCP, and no MCP description or lead names it. Run `npx octocode ghCloneRepo`. File graphs (imports, cycles, dead code) come from the CLI's `npx octocode graph ingest` and `graph query`.
- `clasify` needs a nonblank classification API credential and a successful provider ping; `classification` on four readers needs only the credential. See [availability](OCTOCODE_CLASIFY.md#availability).
- Every tool takes up to 5 `queries` per call. MCP publishes input schemas, titles, and descriptions, not output schemas.
- Server instructions are one prompt of at most 2,000 characters, as a project instruction-size budget; tool-specific parts follow the enabled tool set, and `<clasify>` follows classification availability. The CLI adds a `<cli>` line and the grammar inventory; MCP clients get schemas through `tools/list`.

## Configuration

Set per-client settings in the client `env` block ([example](CONFIGURATION.md#mcp-client-env-block)); env vars beat `.octocoderc` files. Every setting, with defaults and env names: [generated settings](generated/CONFIG_SETTINGS.md). Every tool result carries `structuredContent` and one text block holding its serialized JSON. Development override (`OCTOCODE_NATIVE_BIN`, a candidate binary): [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md#development-environment-variables).

The server shares the CLI's cache under the Octocode home and runs the same best-effort 24-hour sweep at start ([cache storage and lifecycle](CONFIGURATION.md#cache-storage-and-lifecycle)). Unknown or removed tool names in `TOOLS_TO_RUN` and `DISABLE_TOOLS` are ignored with an `unknown_tool_name` config warning; if no `TOOLS_TO_RUN` name is valid, startup fails.

## See also

- [Octocode tools reference](OCTOCODE_TOOLS.md)
- [Octocode CLI guide](OCTOCODE_CLI.md)
- [Security](SECURITY.md)
