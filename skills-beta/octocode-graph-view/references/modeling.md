Load when writing or fixing `model.json`. Why: the graph shows files, imports and calls; only the modeler can name layers, runtime boundaries and flows, and each needs evidence a reader can open.

# Modeling

`model.json` is an optional overlay keyed by file ids from `data.json`. Schema: [model.json](../scheme/model.json). `render.mjs --model` validates it and prints every problem; treat each warning as a model bug.

## Layer taxonomy (suggested ids)

| Layer | Put here | Rule the viewer checks |
|---|---|---|
| `clients` | people, IDEs, agents, browsers (`actor:*` nodes) | callers only |
| `interface` | UI, CLI, MCP server, HTTP routes, extensions | translate and delegate; no business rules |
| `application` | use-case orchestration, pipelines, tool implementations | depends down only |
| `domain` | models, contracts, schemas, policy, pure logic | depends on nothing above |
| `infrastructure` | DB access, HTTP and SDK clients, FFI bindings, process spawners | adapters for the layers above |
| `data` | databases, caches, queues, third-party APIs (`store:*`, `ext:*` nodes) | sinks |
| `tooling` | tests, benchmarks, scripts (`exempt: true`) | ignored by the upward rule |

Match the repo's own layer vocabulary when a doc declares one. Declare `layers` top (callers) to bottom with an `order`, and give each `match` globs (`*` one segment, `**` any) over file ids; the first layer in `order` whose glob matches wins. An import from a larger `order` to a smaller one is derived as `upward`, unless the layer is `exempt` or the pair is listed in `allowed`.

## Evidence recipes by edge kind

| Kind | What proves it | Octocode lane |
|---|---|---|
| `import` | the graph records file:line | `octocode graph query path <from> <to>` for a specific chain |
| `call` | a call site of the target's entry symbol | `lspSearch callers` on the entry; `astSearch` for the call shape |
| `spawn` / `stdio` | `spawn`, `exec`, `Command::new`, binary path resolution | `localSearch` for the spawn call, then `localFetch` of the argv build |
| `ffi` | `#[napi]`, `wasm_bindgen`, `pyo3`, `.node` require or dlopen | `localSearch` + `lspSearch definition` of the loader |
| `http` / `rpc` | base URL plus the client call; for servers, route registration | `localSearch` for host or route, `lspSearch references` of the client |
| `mcp` | `registerTool` or `server.tool` registration and the transport | `localSearch` in the server package |
| `db` | connection open plus a query or migration site | `localSearch` for driver open and SQL, `localFetch` of the schema |
| `event` | publish and subscribe on the same channel name | two `localSearch` hits sharing the literal |
| `config` / `file` | read site of the config, cache or state path | `localSearch` for the path or env key |

Load live tool schemas through `octocode-research` before calling a tool. Import edges alone never prove a runtime flow.

## Overlay moves

- Name a layer for a group of files with `layers[].match`; confirm borderline files with an `assign` entry (`{"id": "<file>", "layer": "<layer>"}`), which wins over globs.
- Add actors, stores and externals as `nodes` with `actor:`, `store:` and `ext:` ids, then link them with `edges` of a runtime `kind`. They draw as squares on the Files view.
- Record a runtime edge the imports cannot show: `{source, target, kind, action, evidence: {path, line}}`.
- When the Hypothesis test in `SKILL.md` dismisses an upward import as legitimate (callback, plugin registration, codegen input), add it to `allowed` with a `reason`.

## Flows

Pick 3-6 scenarios that explain the system: the main request path, one write to state, one external call, one async or background path, and startup or configuration. For each flow, start at the trigger (CLI argv, route, MCP `tools/call`, UI event, cron), follow hops across component boundaries only, record `action` as the concrete call (`POST /orders`, `executeMcp(tool,args)`) with `kind` and `evidence`, and close with the response or side effect. Mark background hops `async: true`. Keep 3-12 steps; split longer flows.

## Summary and findings

Write 3-8 sentences in `meta.summary`: what the system does, the main runtime path, how the layers split responsibilities, where state lives, and the top risks. Findings use `confidence`: `confirmed` (exact code read, `evidence` required), `likely` (strong structural signal) or `candidate` (heuristic until a Hypothesis test confirms it). Only `confirmed` findings should drive action.
