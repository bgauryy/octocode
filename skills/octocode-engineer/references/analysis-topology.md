Load when a graph result (dependents, path, cycle, unreachable code, drift or a caller count) is the lead for a finding, or when a structural signal needs its false explanation ruled out.

# Analysis: graph evidence and controls

## Choose the narrowest evidence lane

Topology triages file relationships; it never proves symbol identity (use semantic references), data/control flow, runtime behavior, or impact.

| Question | Start with | Candidate for |
|---|---|---|
| What a file affects or relies on | graphQuery `dependents`/`deps` (`target`, optional `depth`); `impact` (`target`, or `since` for a diff; rows carry `risk`: `changed` at depth 0, then `direct`, `likely`, `transitive`, `candidate`) | Blast radius, coupling |
| Layer reachability | graphQuery `path` (`target` to `to`), plus the declared rule | Boundary crossing |
| Mutual dependency | graphQuery `cycles` (production import cycles, type-only included; rows `{size,nodes,runtime}`); `issues` with `detectors: ["cycle"]` (each row's `evidence` adds `runtime`, `witness` edges and `suggestedCuts`) | Initialization or change coupling |
| Unreachable or removable | graphQuery `issues` with `detectors: ["unreachable-file","unused-export","test-only"]` and explicit `entries` | Alternate entrypoint, stale code |
| New edges or cycles | graphQuery `drift` (`graph` the head, `baseline` the base; build both trees), or `issues` with `baseline` | Drift |
| Review order | graphQuery `issues` (ranked hypotheses with `evidence` and `controls`) | Hotspot, never a finding |

- Not topology: duplicated policy or leaky interfaces (scenarios, consumers, AST/LSP, history); runtime cost (profile, end-to-end benchmark); slow builds (timed clean/warm runs, I/O, process, cache counts).
- Rust: crate edges from `cargo metadata --format-version 1` (or `cargo tree -e normal`); `rust-best-practices` when installed. `issues` skips `unused-export` and `export-only-local` for Rust (its summary `notes` say so): prove an unused item with `lspSearch` references.
- When discovery lists `graphBuild`, run it once on the smallest directory holding both sides (an unchanged tree reuses its snapshot), then `graphQuery` with the same `path`; `npx -y @octocodeai/octocode-graph schema graphQuery --view variants` lists each operation's fields. Rebuild when the `stale` operation lists changed files. Without graph tools, use `lspSearch` for identity and callers and `astSearch` for shape.
- Reruns: fix the built root, `entries`, and `detectors`.
- Check an `issues` finding against source before acting: `lspSearch` references for an export or symbol, graphQuery `dependents` or `path` for a file or cycle, within the authorized scope.
- For caller counts or absence, combine both tools: either can miss callers. Save the JSON reply of graphQuery `callers` and of `lspSearch` callers for the same declaration (CLI `--json`, or the MCP reply text; one reply per line for each page a top-level `next` returns), then run `node <skill-folder>/scripts/callers-union.mjs graph.json lsp.json` ([callers-union.mjs](../scripts/callers-union.mjs)). Count `callers`: each caller once, `sources` names the tools that found it. Read the call sites of `graphOnly` and `lspOnly` rows and of `sitesDiffer` callers before counting them. `complete: false` keeps the count a lower bound, and `gaps` names why: an unread page, a `partialReasons` entry, an unverified LSP level or unsupported call extraction. Exit 2 names a bad or swapped file. Before an absence claim, also check what neither reply states: consumers outside the scanned roots, `coverage.linkage`, `argumentCoverage` (stats `view: "details"`), the `unresolved` operation's sites and each finding's `tier`. Linkage measures extracted syntax sites, not semantic recall.

## Kill the strongest alternate

| Signal | Common false explanation | Required control |
|---|---|---|
| Cycle | Type-only closing edge; intentionally cohesive module | Exact-read every edge; check `runtime` on the `cycles` row (`false`: no eager module-loading cycle in that group; `"unknown"`: the loop closes only through an edge whose load is not established) and `typesOnly` on `impact` rows; prove implicit edges (`java-same-package`, no import line) with `lspSearch` references |
| Unreachable/dead | Subpath export, CLI/plugin/framework registration, dynamic import, tests or consumers outside the root | Inspect manifests, configs, broader consumers; verify symbols separately |
| Rust file cycle or hub | Sibling modules of one crate (`use super::`, `mod child;`): one compilation unit, no initialization order | Cargo already forbids crate cycles; judge change coupling only, by the declared module order (a boundary test or `ARCHITECTURE.md`), and `cargo metadata` for crate edges |
| Barrel-only export | External or deep-import public contract | Package surface and external consumers (`dependents` rows whose `via` is `reexport`, `named-reexport`, `star-reexport` or a `type-` form of either re-export, file-grouped `lspSearch` references) |
| Cross-folder edge | Folder names imply layers without a rule | Find an authoritative rule or label the boundary inferred |
| Similar code | Distinct rules, trust boundaries, volatility | Compare semantics, edge cases, ownership, change pressure |
| Wide node or long path | Composition root, generated registry, stable shared value, cold path | Inspect responsibilities; measure the claimed cost |
| Small interface | Unsafe call sequence or leaked lifecycle | Consumer workflows, invariants, errors, cancellation, versioning |

- Each control is a [hypothesis test](../SKILL.md#procedure); an alternate left unresolved keeps the finding a candidate.
