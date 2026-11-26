Load when asking who depends on or calls something, a path, cycle, order, region, community or ranking (`impact`, `deps`, `path`, `trace`, `callers`, `critical`, `top`).

# Walks and rankings

## Walk dependencies and calls

| Query | Answer |
|---|---|
| `impact <file...> [depth N]` | Every file that transitively depends on it (reverse breadth-first), each with its depth, the heaviest edge that reached it (`weight`, `through`) and that edge's line; counts per depth and per chunk |
| `deps <file...> [depth N]` / `dependents <file...> [depth N]` | Forward / reverse neighbors; adjacent by default (`depth all` walks everything) |
| `path <a> <b> [--hops]` | The strongest dependency path a → b (Dijkstra, cost Σ 1/weight); add `--hops` for a minimum-hop path on the same edges (BFS). Both return the weight, import lines and symbol calls at each hop. In the viewer, **Compare fewest hops** reruns the weighted query with its filters. No path: says so and checks b → a |
| `path <a> <b> --all [n\|all]` | One route per distinct first hop of `a` that still reaches `b`, fewest hops first: every way `a` pulls `b` in. Text marks non-loading hops `→[type]`, `→[deferred]` |
| `path <a> <b> --avoid <file\|dir>...` | An architecture rule as a path: a route that passes none of the avoided files (a directory avoids every file under it) is a violation of a rule that every route must cross them, with each hop's line; none means the rule holds. |
| `region <entry> <outcome>` | Everything that can take part in a route from the entry to the outcome: reachable forward from one and backward from the other. Files by default; two `file#symbol` arguments ask the call graph. Rows carry `fromEntry` and `toOutcome` hops and `mustPass`; `stats.mustPass` lists the steps on every modeled route (the outcome's dominators). |
| `order <file>` | Prerequisite-first order for the seed and its reachable dependencies. Files in a strongly connected component share a group. `crossings` contains the evidence for every edge between groups. Add `--imports` for an import-only order. |
| `trace <seed> <pattern>` | Match typed walks from a file, symbol, or provider entity. Juxtaposition or `·` means sequence, `\|` means choice, `*` includes zero steps and `+` requires at least one. Parentheses group expressions. Add `--depth N` to bound the whole pattern to N relationships, including zero. Example: `trace src/main.ts DEFINES (CALLS)+`. Native export relations are `DEFINES`, `IMPORTS` and `CALLS`. JSON returns each matching endpoint and every witness on an accepting walk. [Additional provider relations](providers.md) |
| `cycle [<file>]` | With a file: its strongly connected set under the selected filters, with its internal weight and one shortest ring through the file, starting at the file; among rings of equal length, the next hop with the smallest id wins. Other members follow in PageRank order. A `metrics.cycles` ID identifies an exact match of the member set. With no file: every cycle the build measured, largest first, then by smallest member, each with all its members. A file that imports itself is a one-member cycle. |
| `community <file\|id>` | Members by PageRank and the weight to every other community (`links`, out and in) |
| `callers <sym>` / `callees <sym>` | Symbol-level calls with the saved `via` value, confidence and lines. Look in `data.json` for `resolution: ["stitched-import"]` to identify calls bound by name across chunks. `via: scip` marks a Rust call that a rust-analyzer SCIP index resolved; that index runs only if the user's own rust-analyzer settings enable build scripts and proc macros. Use `file#name` (any file form) or a bare symbol name. With `--tests`, `callers` adds `stats.testCalls` and `stats.testFiles`: the callers in test files. |
| `critical functions [n\|all]` | Functions ranked by PageRank over the call graph weighted by call sites × evidence, each with `callSites`, `callers`, `callerFiles` and `transitiveCallers`. |
| `critical points [n\|all]` | Files whose removal disconnects the production file graph (articulation points): `value` is the files cut off from the rest of the component, `parts` the sizes left. |
| `critical path [<file>] [--up]` | The longest dependency chain below a file (with `--up`, the longest chain of dependents above it); without a file, the deepest chain in the graph. Rows carry each hop's weight, import lines and calls, like `path`. |
| `top <metric> [n\|all]` | Files ranked by `pagerank`, `betweenness`, `inWeight`, `outWeight`, `core` or `depth`; use `all` for the full ranking and check the summary's coverage |

For a change spanning several files, pass the paths to one `impact`, `deps` or `dependents` query. Each reached file appears once, at its minimum depth from any input. Seeds stay in the highlight and are excluded from result rows. Ask separate queries when you need attribution for every seed.

A `callers` answer is complete only when every callable was probed and every probe answered (`--lsp all`). Any other `callers` answer is `isPartial` with `partialReasons: ["callersNotProbed"]`, carries `warnings` (what the graph cannot see: calls from other chunks are bound only through stitched imports and the build's LSP probes; calls inside Rust macros reach only the language server) and `confirm`, a ready `lspSearch` call for the same callers. Run it before claiming a complete caller list.
