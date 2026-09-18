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

`createNativeMcp()` constructs one `NativeRuntime` and calls `catalog()`. The
adapter omits tools with `available: false`. For every available tool, it:

1. looks up the matching core-owned Standard Schema definition;
2. registers its title, description, input/output schemas, and annotations;
3. forwards the unchanged MCP arguments to `NativeRuntime.executeMcp()`;
4. forwards MCP cancellation to `NativeRuntime.cancel()`.

A catalog entry without a matching registration schema is a startup error. This
prevents the advertised native catalog and the MCP surface from drifting.

## Lifecycle

`startNativeMcp()` connects `StdioServerTransport`. Its returned `close()` method
is idempotent and closes the transport before the native runtime. The native
runtime owns request admission, cancellation, worker cleanup, caches, LSP
clients, and other execution resources.

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
