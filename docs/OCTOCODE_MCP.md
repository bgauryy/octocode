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

Results use `results: [{index, data, status?, rowPart?}]`. Required work appears under `next` as `{kind:"continue", tool, input:{cursor}}`; optional exploration appears under `related` as `{kind:"related", tool, input:{queries:[...]}}`, and may widen scope (hidden or ignored paths). Copy the call's `input` unchanged to its named tool. `hints` contain explanatory text. A failed row has `status:"error"` with `data.errorCode` (and `httpStatus` for provider failures); the process exit code is not per row. Paging keys and the completeness rule are in the [tools reference](OCTOCODE_TOOLS.md#results-evidence-partial-failures-and-continuations).

## Tool catalog

The contract has 16 tools. [Configuration](CONFIGURATION.md#tool-availability-cli-and-mcp) owns the default MCP catalog, graph/clone/classification opt-ins, and additional tool filters. Tools excluded by configuration are omitted from `tools/list`. A configured provider can still fail when invoked; discovery does not probe its health.

| Family | Tools |
|--------|-------|
| GitHub | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` |
| Local | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, `lspSearch`, `graphBuild`, `graphQuery` |
| Package | `artifactSearch` |
| Semantic assessment | `clasify` |

- `ghCloneRepo` writes a fresh checkout on every call; `graphBuild` writes and prunes derived snapshots under workspace `.octocode/graph`. Their MCP annotations reflect those writes. `graphQuery` reads snapshots. The implementations are shared with the CLI.
- `clasify` and inline `classification` use a configured provider credential. Questions, source capture and recovery are covered by the [classification guide](OCTOCODE_CLASIFY.md); exposure is covered by the configuration matrix above.
- Tools take up to 5 `queries` per call; `graphBuild` accepts one. MCP publishes input schemas, titles, and descriptions, not output schemas.
- Server instructions are one static prompt within the `MAX_MCP_INSTRUCTION_BYTES` UTF-8 limit in `@octocodeai/octocode-core/mcp`, a project instruction-size budget: cross-tool order, envelope, paging and evidence rules. Tool-specific rules, output (`Returns:`) notes and clasify guidance live in each tool's description, so they arrive only with the listed tool. The CLI adds a `<cli>` line and the grammar inventory; MCP clients get schemas through `tools/list`.

## Configuration

Start with [Configuration](CONFIGURATION.md) for setup, precedence and the complete settings reference. Its [client `env` example](CONFIGURATION.md#mcp-client-env-block) applies settings to one MCP process. Every tool result carries `structuredContent` and one text block holding its serialized JSON. Development override (`OCTOCODE_NATIVE_BIN`, a candidate binary): [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md#development-environment-variables).

The server shares the CLI's cache under the Octocode home and runs the same best-effort 24-hour sweep at start ([cache storage and lifecycle](CONFIGURATION.md#cache-storage-and-lifecycle)). Unknown or removed tool names in `TOOLS_TO_RUN` and `DISABLE_TOOLS` are ignored with an `unknown_tool_name` config warning; if no `TOOLS_TO_RUN` name is valid, startup fails.

## See also

- [Octocode tools reference](OCTOCODE_TOOLS.md)
- [Octocode CLI guide](OCTOCODE_CLI.md)
- [Security](SECURITY.md)
