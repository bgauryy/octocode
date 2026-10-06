# Octocode MCP architecture

`octocode-mcp` is a thin stdio adapter around `@octocodeai/octocode-native`.
It owns transport registration and process startup. It does not implement,
select, sanitize, or execute tools in TypeScript.

## Runtime boundary

- `src/index.ts` starts the native MCP adapter and reports startup failures.
- `src/native/index.ts` loads `NativeRuntime`, reads its catalog, registers the
  enabled tools with MCP SDK v2, forwards each call to `executeMcp()`, forwards
  cancellation, and closes the transport before it closes the runtime.
- `src/public.ts` exposes typed wrappers for embedding the same native adapter.
- `@octocodeai/config/schema` supplies the core-owned Standard Schema objects needed
  by `McpServer.registerTool()`. Public contracts remain owned by core.
- `@octocodeai/octocode-native` owns configuration, policy, validation,
  execution, response shaping, sanitization, pagination, and shutdown of tool
  resources.

There is no TypeScript tool fallback or runtime selector. If the native binding is
missing or invalid, startup fails closed.

## Tool registration

The canonical/native catalog contains sixteen tools. MCP can register thirteen
read tools when all availability gates pass; it registers only the available
subset. `ghCloneRepo`, `astTopology` and `astRewrite` are CLI-only (core policy
`cliOnly`; `OCTOCODE_BETA` gates the last two on the CLI): the native catalog
marks them `unavailableReason: "cliOnly"` on MCP and rejects direct calls, and
`clasify` requires a nonblank `OCTOCODE_CLASSIFICATION_API`.

`createNativeMcp()` constructs one `NativeRuntime`, awaits
`probeClassification()`, then calls `catalog()`. When clasify is available,
the probe sends one minimal judgment (5 s cap, one retry); any failure except a
rate limit makes native drop clasify from the catalog (`unavailableReason:
"providerUnreachable"`) and the adapter logs the reason to stderr. The
adapter omits tools with `available: false`; this keeps both default-off tools
out of MCP discovery rather than advertising unusable contracts. For every
available tool, it:

1. looks up the matching core-owned Standard Schema definition;
2. registers its title, description, and core-owned published input schema; MCP discovery intentionally
   omits annotations and output schemas while tool results remain structured;
3. runs native lossless input normalization before canonical Standard Schema
   validation, then forwards the validated arguments to `NativeRuntime.executeMcp()`;
4. forwards MCP cancellation to `NativeRuntime.cancel()`.

A catalog entry without a matching registration schema is a startup error. This
prevents the advertised native catalog and the MCP surface from drifting.

The published input schema is a compact superset for agent discovery, not the
validator. Canonical core schemas remain strict: malformed encoded arrays and
invalid list items are rejected. Valid JSON-encoded arrays and supported scalar
strings are normalized by the same native path used by CLI. Mixed bulk calls
with valid rows preserve indexed invalid-row errors rather than dropping rows.

Shared agent guidance is returned once in MCP `initialize.instructions`, built
by core through `@octocodeai/config/mcp` for the available tools. `tools/list`
descriptions contain only tool-specific guidance; never prefix them with the
server instructions. A client may project server instructions into its model
context differently, so inspect the raw MCP response before attributing repeated
host metadata to server registration.

## Lifecycle

`startNativeMcp()` connects `StdioServerTransport`. Its returned `close()` method
is idempotent and closes the transport before the native runtime. The native
runtime owns request admission, cancellation, worker cleanup, caches, LSP
clients, and other execution resources. MCP constructs that runtime with a
300-second execution deadline, matching CLI and staying strictly above the
configured cold-start-plus-one-request budget: initialize, Java's 120-second
readiness wait, request retries, retry delays, and transport overhead. The same
outer cap still bounds multi-request hierarchy traversals.

## Distribution

The package ships the bundled JavaScript adapter, bundled public declarations,
and `dist/docs/`. The native root package selects its matching platform addon at
installation time.

`package.json` defines the build and verification entry points. The real
boundary checks are:

- `tests/native/node-boundary.mjs` for direct adapter registration, execution,
  filtering, and close behavior;
- `tests/integration/stdio.acceptance.mjs` for the built stdio server;
- native Rust tests under `packages/octocode-native` for tool behavior and
  safety contracts.

Acceptance fixtures live in a normal workspace subtree and are removed after
the run; placing them under a Git-ignored directory would test ignore rules
instead of discovery and rewriting.

Publish the native platform packages and native root package before publishing
`octocode-mcp`.
