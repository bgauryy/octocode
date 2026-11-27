Load when naming architecture layers, recording runtime edges or flows the imports cannot show, or validating `model.json`.

# Modeling

Use this optional overlay to name architectural boundaries and document runtime flows. It uses file IDs from `data.json`. Validate the overlay with `npx -y @octocodeai/octocode-graph render --data <data.json> --model <model.json>` (each problem prints to stderr) and the model schema exported as `@octocodeai/octocode-graph/model-schema`.

## Layers

Use the repository’s vocabulary and model the boundary being investigated. Unmatched files show the scope the overlay leaves uncovered. Declare `layers` top (callers) to bottom with an `order`, and give each `match` globs (`*` one segment, `**` any) over file ids; the first layer in `order` whose glob matches wins. An import from a larger `order` to a smaller one is derived as `upward`, unless the layer is `exempt` or the pair is listed in `allowed`.

## Evidence recipes by edge kind

| Kind | What proves it | Octocode lane |
|---|---|---|
| `import` | the graph records file:line | graphQuery `path` (`target` to `to`) for a specific chain |
| `call` | a call site of the target's entry symbol | `lspSearch callers` on the entry; `astSearch` for the call shape |
| `spawn` / `stdio` | `spawn`, `exec`, `Command::new`, binary path resolution | `localSearch` for the spawn call, then `localFetch` of the argv build |
| `ffi` | `#[napi]`, `wasm_bindgen`, `pyo3`, `.node` require or dlopen | `localSearch` + `lspSearch definition` of the loader |
| `http` / `rpc` | base URL plus the client call; for servers, route registration | `localSearch` for host or route, `lspSearch references` of the client |
| `mcp` | `registerTool` or `server.tool` registration and the transport | `localSearch` in the server package |
| `db` | connection open plus a query or migration site | `localSearch` for driver open and SQL, `localFetch` of the schema |
| `event` | publish and subscribe on the same channel name | two `localSearch` hits sharing the literal |
| `config` / `file` | read site of the config, cache or state path | `localSearch` for the path or env key |

A static import establishes a dependency. A runtime flow also needs evidence of the action at that boundary.

## Overlay moves

- Name a layer for a group of files with `layers[].match`; confirm borderline files with an `assign` entry (`{"id": "<file>", "layer": "<layer>"}`), which wins over globs.
- Add actors, stores and externals as `nodes` with `actor:`, `store:` and `ext:` ids, then link them with `edges` of a runtime `kind`. The brief lists them with the model; the live viewer draws the graph's own nodes only.
- Record a runtime edge the imports cannot show: `{source, target, kind, action, evidence: {path, line}}`.
- For an intended callback, plugin registration or codegen dependency that crosses the declared layer order, use `allowed` with the source-backed reason. [Evidence](evidence.md)

## Flows

Pick the scenarios the requested scope contains, for example the main request path, a write to state, an external call, an async or background path, or startup and configuration; never invent a flow the code does not have. For each flow, start at the trigger (CLI argv, route, MCP `tools/call`, UI event, cron), follow hops across component boundaries only, record `action` as the concrete call (`POST /orders`, `executeMcp(tool,args)`) with `kind` and `evidence`, and close with the response or side effect. Mark background hops `async: true`. Split a flow when separate boundaries become hard to follow; retain every evidenced hop.

## Summary and findings

Use `meta.summary` to explain the modeled boundary. Findings take `confidence` values of `confirmed` (source evidence required), `likely` (structural evidence) or `candidate` (a hypothesis to investigate).
