# Octocode MCP architecture

`octocode-mcp` is a thin stdio adapter around `@octocodeai/octocode-native`.
It owns transport registration and process startup. It does not implement,
select, sanitize, or execute tools in TypeScript.

## Runtime boundary

- `src/index.ts` starts the native MCP adapter and reports startup failures.
- `src/native/index.mjs` loads `NativeRuntime`, reads its catalog, registers the
  enabled tools with MCP SDK v2, forwards each call to `executeMcp()`, forwards
  cancellation, and closes the transport before it closes the runtime.
- `src/public.ts` exposes typed wrappers for embedding the same native adapter.
- `@octocodeai/octocode-core/schema` supplies the Standard Schema objects needed
  by `McpServer.registerTool()`. Public contracts remain owned by core.
- `@octocodeai/octocode-native` owns configuration, policy, validation,
  execution, response shaping, sanitization, pagination, and shutdown of tool
  resources.

There is no TypeScript tool fallback or runtime selector. If the native binding is
missing or invalid, startup fails closed.

## Tool registration

The native catalog contains twelve tools. MCP registers only the available
subset: `astRewrite` requires `ENABLE_AST_REWRITE=true`, and `semanticAssess`
requires a nonblank `OCTOCODE_JEV_KEY`.

`createNativeMcp()` constructs one `NativeRuntime` and calls `catalog()`. The
adapter omits tools with `available: false`; this keeps both default-off tools
out of MCP discovery rather than advertising unusable contracts. For every
available tool, it:

1. looks up the matching core-owned Standard Schema definition;
2. registers its title, description, input schema, and annotations; MCP discovery
   intentionally omits output schemas while tool results remain structured;
3. forwards the unchanged MCP arguments to `NativeRuntime.executeMcp()`;
4. forwards MCP cancellation to `NativeRuntime.cancel()`.

A catalog entry without a matching registration schema is a startup error. This
prevents the advertised native catalog and the MCP surface from drifting.

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

Publish the native platform packages and native root package before publishing
`octocode-mcp`.
