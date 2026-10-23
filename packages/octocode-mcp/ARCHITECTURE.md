# Octocode MCP architecture

`octocode-mcp` is a thin stdio adapter around `@octocodeai/octocode-native`.
It owns transport registration and process startup. It does not implement,
select, sanitize, or execute tools in TypeScript.

## Runtime boundary

- `src/index.ts` starts the native MCP adapter and reports startup failures.
- `src/native/index.ts` loads `NativeRuntime`, reads its catalog, registers the
  enabled tools with MCP SDK v2, forwards each call to `executeMcp()`, forwards
  cancellation, and closes the runtime before it closes the transport. A typed
  native failure (timeout, busy, tool unavailable) comes back as the same
  `{error, errorCode}` tool error the CLI prints; the addon has already scrubbed
  it. Any other rejection stays a generic "failed to execute" error with the
  detail on stderr.
- `src/native/schema.ts` builds each tool's registered schema: core's published
  input view wrapped as a pass-through Standard Schema.
- `src/public.ts` re-exports the adapter functions and types for embedding;
  the runtime types come from `@octocodeai/octocode-native/runtime`.
- `@octocodeai/config` (private workspace package, bundled at build time)
  supplies the core-owned tool definitions, published input schemas, and server
  instructions. Public contracts remain owned by core, which stays an external
  runtime dependency.
- `@octocodeai/octocode-native` owns configuration, policy, validation,
  execution, response shaping, sanitization, pagination, and shutdown of tool
  resources.

There is no TypeScript tool fallback or runtime selector. If the native binding is
missing or invalid, startup fails closed.

## Tool registration

The native catalog has fourteen tools. On MCP the runtime marks the CLI-only
`ghCloneRepo` `unavailableReason: "cliOnly"` and refuses
direct calls; `clasify`
needs `OCTOCODE_BETA` and a nonblank `OCTOCODE_CLASSIFICATION_API`. That leaves
twelve tools by default and thirteen with `clasify`.

`createNativeMcp()` constructs one `NativeRuntime`, checks its ABI version and
contract fingerprint, and reads `catalog()` without contacting the
classification provider. It skips tools with `available: false`. For every
available tool, it:

1. looks up the matching core-owned tool definition;
2. registers its title, description, core-authored annotations (such as
   `readOnlyHint`), and core's published input schema as a pass-through
   Standard Schema: the SDK advertises the JSON schema and does not validate;
   `tools/list` carries no output schemas, while results stay structured;
3. forwards the raw arguments to `NativeRuntime.executeMcp()`, which validates
   them once against the canonical contract and returns the repair message for
   a rejected call;
4. forwards MCP cancellation to `NativeRuntime.cancel()`.

Tools named in `mcp.deferred` (`OCTOCODE_DEFER_TOOLS`) stay available but leave
`tools/list`; one `run` tool executes them from a `{tool, query}` lead.

A catalog entry without a matching registration schema is a startup error. This
prevents the advertised native catalog and the MCP surface from drifting.

The published input schema is a compact superset for agent discovery, not the
validator. Canonical core schemas remain strict: encoded arrays, invalid list
items, and strings supplied for number or boolean fields are rejected by the
same native path used by CLI. Mixed bulk calls
with valid rows preserve indexed invalid-row errors rather than dropping rows.

Shared agent guidance is returned once in MCP `initialize.instructions`: one
core prompt (through `@octocodeai/config/mcp`) for every tool subset. `tools/list`
descriptions contain only tool-specific guidance; never prefix them with the
server instructions. A client may project server instructions into its model
context differently, so inspect the raw MCP response before attributing repeated
host metadata to server registration.

## Lifecycle

`startNativeMcp()` connects `StdioServerTransport`. Its returned `close()` method
is idempotent and closes the native runtime before the transport. The native
runtime owns request admission, cancellation, worker cleanup, caches, LSP
clients, and other execution resources, including the 300-second per-request
execution budget every surface shares. On SIGINT, SIGTERM or stdin end, the
server cancels in-flight requests and exits after the runtime closes, or after
a 10-second grace period.

## Distribution

The package ships the bundled JavaScript adapter, bundled public declarations,
and `dist/docs/`. The native root package selects its matching platform addon at
installation time.

`package.json` defines the build and verification entry points. The real
boundary checks run against the built package:

- `yarn test:boundary` (`tests/native/node-boundary.mjs`) for direct adapter
  registration, execution, filtering, and close behavior;
- `yarn test:acceptance` (`tests/integration/stdio.acceptance.mjs`) for the
  built stdio server and CLI; the runner takes its shared client and receipt
  from `stdio-harness.mjs` and its checks from the `stdio-*-checks.mjs` modules;
- native Rust tests under `packages/octocode-native` for tool behavior and
  safety contracts.

Acceptance fixtures live in a normal workspace subtree and are removed after
the run; placing them under a Git-ignored directory would test ignore rules
instead of discovery and rewriting.

Publish the native platform packages and native root package before publishing
`octocode-mcp`.
