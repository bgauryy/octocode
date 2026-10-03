# Modeling

Load when writing or fixing `model.json`. Why: the scan sees files and imports; only the modeler can name roles, runtime boundaries, and flows, and each needs evidence a reader can open.

## Layer taxonomy (default ids)

| Layer | Put here | Rule the viewer checks |
|---|---|---|
| `clients` | people, IDEs, agents, browsers (`actor:*`) | callers only |
| `interface` | UI, CLI, MCP server, HTTP routes, extensions, webhooks | translate and delegate; no business rules |
| `application` | use-case orchestration, pipelines, jobs, tool implementations | depends down only |
| `domain` | models, contracts, schemas, policy, pure logic | depends on nothing above |
| `infrastructure` | DB access, HTTP/SDK clients, FFI bindings, process spawners, engines | adapters for the layers above |
| `data` | databases, caches, queues, files, third-party APIs (`store:*`, `ext:*`) | sinks |
| `tooling` | tests, benchmarks, scripts (`exempt: true`) | ignored by upward/cycle rules |

Match the repo's own layer vocabulary when a doc declares one. An edge from a larger `order` to a smaller one is flagged `upward`.

## Evidence recipes by edge kind

| Kind | What proves it | Octocode lane |
|---|---|---|
| `import` | scan already records file:line | trust scan; `astTopology path` for a specific chain |
| `call` | a call site of the target's entry symbol | `lspSearch callers`/`callHierarchy` on the entry; `astSearch` for the call shape |
| `spawn` / `stdio` | `spawn`, `exec`, `Command::new`, binary path resolution | `localSearch` for the spawn call, then `localFetch` of the argv build |
| `ffi` | `#[napi]`, `wasm_bindgen`, `pyo3`, `.node` require or dlopen | `localSearch` + `lspSearch definition` of the loader |
| `http` / `rpc` | base URL plus the client call; for servers, route registration | `localSearch` for host or route, `lspSearch references` of the client |
| `mcp` | `registerTool`/`server.tool` registration and the transport | `localSearch` in the server package |
| `db` | connection open plus a query or migration site | `localSearch` for driver open and SQL, `localFetch` of the schema |
| `event` | publish and subscribe on the same channel name | two `localSearch` hits sharing the literal |
| `config` / `file` | read site of the config, cache, or state path | `localSearch` for the path or env key |

Load live tool schemas through `octocode-research` before calling a tool.

## Overlay moves

- Confirm a scan node: restate `id` with `kind`, `layer`, `label`, `description`, and `evidence`. Overlay nodes default to `guess: false`.
- Add actors, stores, and externals with `actor:`, `store:`, and `ext:` ids. Link them with typed edges.
- Fold noise: `{"id": "<child>", "mergeInto": "<parent>"}` sums files and loc and redirects edges. Use `exclude` globs for fixtures.
- Remove a false-positive edge or node with `remove: true` (for example, a URL found in a generated contract).
- Mark a legitimate upward edge (callback, plugin registration, codegen input) with `allowed: true`.

## Flows

Pick 3–6 scenarios that explain the system: the main request path, one write to state, one external call, one async or background path, and startup or configuration. For each flow, start at the trigger (CLI argv, route, MCP `tools/call`, UI event, cron), follow hops across component boundaries only, record `action` as the concrete call (`POST /orders`, `executeMcp(tool,args)`) with `kind` and evidence, and close with the response or side effect. Mark background hops `async: true`. Keep 3–12 steps; split longer flows.

## Explanation (`meta.summary`)

Write 3–8 sentences or bullets: what the system does, the main runtime path, how the layers split responsibilities, where state lives, and the top risks. Add `meta.stack` chips. Findings use `confidence`: `confirmed` (exact code read), `likely` (strong structural signal), or `candidate` (heuristic). Only `confirmed` findings should drive action.
