# Octocode MCP server

The Octocode MCP server exposes Octocode's research tools to AI coding clients over stdio. It is a thin adapter: it registers schemas and frames the protocol, while `@octocodeai/octocode-native` owns tool behavior, configuration, security policy, providers, credentials, caches, and stats. MCP and `npx octocode <toolName> '<json>'` share the same schemas, runners, validation, response envelope, pagination, and secret redaction.

Tool fields and the shared call contract (batch envelope, `mainGoal`/`reasoning`, partial failures, `next` pages, `hints` leads): [How every tool call works](OCTOCODE_TOOLS.md#how-every-tool-call-works). Settings: [CONFIGURATION.md](CONFIGURATION.md). Tokens: [AUTHENTICATION.md](AUTHENTICATION.md).

The MCP server requires Node.js and loads the native addon. The standalone `octocode` binary exposes CLI commands; it does not serve MCP. Public Agent Skills are installed through the CLI’s embedded bundle, not an MCP tool. See [CLI interfaces](OCTOCODE_CLI.md) and [skill installation](OCTOCODE_CLI.md#skill--agent-skills).

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
- Settings and env tokens resolve once at startup. Stored logins and `gh` are resolved by native credential discovery; see [credential refresh and caching](AUTHENTICATION.md).

Native validates arguments once, as on the CLI, and an invalid call gets the same repair guidance (nearest field, allowed values, the missing field's fix). A call is `{queries:[...]}`; one bare query object runs as one row, and a bare array is rejected. A JSON-encoded (curly quotes or HTML character references such as `&quot;` included) or bare-scalar value for a list-only field, or an exact integer or boolean string for an integer- or boolean-only field, is repaired; fields that also accept a string are never rewritten. In a batch, valid rows run and each invalid row returns its own error.

## Tool catalog

The contract has 14 tools. With default settings MCP registers 12; beta plus a classification credential adds `clasify` (13). Unavailable tools are left out of `tools/list`, not registered as failing calls.

| Family | Tools |
|--------|-------|
| GitHub | `ghSearchRepo`, `ghSearchCode`, `ghStructure`, `ghGetFileContent`, `ghSearchHistory`, `ghGetHistoryItem`, `ghCloneRepo` (CLI-only) |
| Local | `localSearch`, `localFetch`, `structureSearch`, `astSearch`, `lspSearch` |
| Package | `artifactSearch` |
| Semantic assessment | `clasify` |

- `ghCloneRepo` is never registered on MCP, and no MCP description or lead names it. Run `npx octocode ghCloneRepo`. File graphs (imports, cycles, dead code) come from the CLI's `npx octocode graph ingest` and `graph query`.
- `clasify` needs beta and a nonblank classification API credential; `clasifyContext` on four readers needs only the credential. See [availability](OCTOCODE_CLASIFY.md#availability).
- Every tool takes up to 5 `queries` per call. MCP publishes input schemas, titles, and descriptions, not output schemas.
- Server instructions are one prompt of at most 2,000 characters, as a project instruction-size budget; tool-specific parts follow the enabled tool set, and `<clasify>` follows classification availability. The CLI adds a `<cli>` line and the grammar inventory; MCP clients get schemas through `tools/list`.
- With `mcp.deferred` (`OCTOCODE_DEFER_TOOLS`), the listed tools leave `tools/list` and one `run` tool executes them.

## Configuration

Set per-client settings in the client `env` block ([example](CONFIGURATION.md#mcp-client-env-block)); env vars beat `.octocoderc` files. Every setting, with defaults and env names: [generated settings](generated/CONFIG_SETTINGS.md). Every tool result carries `structuredContent` and one text block holding its serialized JSON. Development overrides (`OCTOCODE_NATIVE_BINDING`, `OCTOCODE_ALLOW_CONTRACT_DRIFT`): [DEVELOPMENT.md](../skills-dev/octocode-dev/docs/DEVELOPMENT.md#development-environment-variables).

The server shares the CLI's cache under the Octocode home and runs the same best-effort 24-hour sweep at start ([cache storage and lifecycle](CONFIGURATION.md#cache-storage-and-lifecycle)). Unknown or removed tool names in `TOOLS_TO_RUN` and `DISABLE_TOOLS` are ignored with an `unknown_tool_name` config warning; if no `TOOLS_TO_RUN` name is valid, startup fails.

## See also

- [Octocode tools reference](OCTOCODE_TOOLS.md)
- [Octocode CLI guide](OCTOCODE_CLI.md)
- [Security](SECURITY.md)
