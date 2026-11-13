Load when modeling a design, boundary, interface, or flow, or when topology, cycles, reachability, dead code, duplication, coupling, or efficiency affects a decision.

# Analysis

## Model the wiring

- Name each hop's interface ([contract trace](checks.md#contract-trace)); make invalid states unrepresentable; keep core decisions apart from UI, storage, transport.
- Record `module/layer -> responsibility -> owned data/invariants -> public interface -> allowed dependencies`; name each role and style with [concepts](concepts.md#use-a-concept-in-a-review).
- Cross-check views; each hides what another exposes: static (file/symbol edges), control (who initiates, retries, cancels, handles errors), data ([data-flow trace](checks.md#data-flow-trace)), ownership (invariants, lifecycles, contracts), runtime (config, registries, processes, generated artifacts, dynamic dispatch).
- For a bounded decision, rejoin views for representative scenarios as `trigger -> decision -> transformation -> side effect -> result/error`; note unresolved reflection, queues, registration, external systems, trust and transaction boundaries. For a whole-system audit, cover every inventoried surface and junction rather than sampling flows; `octocode-workflows`, when installed, provides a traversal procedure.
- Name the quality attribute judged (correctness, security, changeability, performance, operability); [kill the strongest alternate](#kill-the-strongest-alternate) for look-alike signals.
- Enforce layering in the build: a boundary test over the module tree, a `cargo metadata` check or `cargo deny` `[bans]` (`deny.toml`) for crate edges, a dependency lint for TypeScript. Read the rule back from that file; a prose-only rule is `inferred`.
- Generated source of truth (contract, schema, rule table): assert each hand-copied string (messages, allowlists, defaults) exists in the generated source; outcome parity tests miss copied text.

## Decompose large tasks

- Split into the smallest verifiable problems, each a Task contract: objective, inputs, constraints, output, Definition of Done (the verification point); order by dependency edges.
- Parallelize only with clear ownership and a latency or independent-evidence gain, never to hide failures.
- Compose after parts pass; test shared interfaces end to end.

## Explore the graph and find critical paths

- Seed exploration from exposed tools, user flows, protocol ingress, state owners, lifecycle hooks, and observed failures or profiles. Reconcile the graph with runtime registration; review the whole requested inventory, not only the graph's highest-ranked issues.
- Expand breadth through callers, dependents, dependencies, implementations, and consumers. For each junction, ask what enters, what branches, what state changes, and who consumes the result. Keep unresolved dynamic or external edges visible.
- Follow depth from a concrete trigger to its effect and observable outcome, then backtrack through error, fallback, retry, cancellation, and cleanup branches. Include asynchronous callbacks, queues, and fan-out/fan-in; an import chain alone is not an execution path.
- Use `astSearch` to locate candidate structures and `lspSearch` to resolve decisive identities. Preserve a frontier of unexplored edges and deduplicate by source identity and relevant scenario; record cycles without walking them forever.
- Distinguish correctness-critical paths from latency-critical paths. A rarely used recovery or authorization path can be critical; high fan-in is only a lead for a runtime hotspot. Establish the workload and baseline before ranking measured cost.
- At each critical path, apply the [review angles](checks.md#review-angles), record healthy invariants and disproved suspicions, and identify the next check that could change the verdict. Trace a changed boundary's upstream and downstream effects before closing it.

## Choose the narrowest evidence lane

Topology triages file relationships; it never proves symbol identity (use semantic references), data/control flow, runtime behavior, or impact.

| Question | Start with | Candidate for |
|---|---|---|
| What a file affects or relies on | graphQuery `dependents`/`deps` (`target`, optional `depth`); `impact` (`target`, or `since` for a diff) | Blast radius, coupling |
| Layer reachability | graphQuery `path` (`target` to `to`), plus the declared rule | Boundary crossing |
| Mutual dependency | graphQuery `cycles` (production import cycles, type-only included; rows `{size,nodes,runtime}`); `issues` with `detectors: ["cycle"]` (each row's `evidence` adds `runtime`, `witness` edges and `suggestedCuts`) | Initialization or change coupling |
| Unreachable or removable | graphQuery `issues` with `detectors: ["unreachable-file","unused-export","test-only"]` and explicit `entries` | Alternate entrypoint, stale code |
| New edges or cycles | graphQuery `drift` (`graph` the head, `baseline` the base; build both trees), or `issues` with `baseline` | Drift |
| Review order | graphQuery `issues` (ranked hypotheses with `evidence` and `controls`) | Hotspot, never a finding |

- Not topology: duplicated policy or leaky interfaces (scenarios, consumers, AST/LSP, history); runtime cost (profile, end-to-end benchmark); slow builds (timed clean/warm runs, I/O, process, cache counts).
- Rust: crate edges from `cargo metadata --format-version 1` (or `cargo tree -e normal`); `rust-best-practices` when installed. `issues` skips `unused-export` and `export-only-local` for Rust (its summary `notes` say so): prove an unused item with `lspSearch` references.
- When discovery lists `graphBuild`, run it once on the smallest directory holding both sides (an unchanged tree reuses its snapshot), then `graphQuery` with the same `path`; `node packages/octocode/out/octocode.js schema graphQuery --view variants` lists each operation's fields. Rebuild when the `stale` operation lists changed files. Without graph tools, use `lspSearch` for identity and callers and `astSearch` for shape.
- Reruns: fix the built root, `entries`, and `detectors`.
- Check an `issues` finding against source before acting: `lspSearch` references for an export or symbol, graphQuery `dependents` or `path` for a file or cycle, within the authorized scope.
- For caller counts or absence, union graphQuery `callers` with `lspSearch` callers: either tool can miss callers. Before an absence or completeness claim, check consumers outside the scanned roots, graph scan gaps, `coverage.callExtraction`, `coverage.linkage`, `argumentCoverage` (stats `view: "details"`), the `unresolved` operation's sites and each finding's `tier`. Linkage measures extracted syntax sites, not semantic recall. Follow emitted continuations and recovery commands; check LSP coverage independently. Partial LSP results remain a lower bound until the missing scope is resolved.

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

## Claim limits

- Label numbers measured, documented or estimated. Complexity is not latency; locality, I/O, contention, allocation and network work can dominate. Version-specific claims need current documentation or source.
- State approximation error and permitted consistency anomalies beside the choice. Concurrent and memory-safety claims need an execution check, such as a sanitizer, stress test or model checker.
- Match [evidence](evidence.md) to the claim. Distinguish fixture IDs, paths, versions and roots; assert the specific failure. Unresolved scope, identity, flow, runtime impact or measurement keeps a finding a candidate.

## Proof and ranking

- Use the narrowest decisive evidence: source and semantic references for identity, scenario traces for ownership and effects, and execution or measurement for behavior and cost. Graph candidates do not require every tool in sequence.

- Affected scope: graph dependents/path plus semantic references to the changed symbol.
- Layer violation: exact path, importing symbol, authoritative rule, quality impact.
- Dead code / delete: the [removal proof](change-discipline.md#delete-junk-and-unify).
- Coupling or god module: graph breadth plus mixed responsibilities and real consumer or change impact, not size.
- Duplication: semantic equivalence, shared invariant, shared change pressure, not text similarity.
- Separation/interface: inward framework/storage/view dependency, invariant owner, consumer contract, policy test seam.
- Hot path: named workload and budget; end-to-end baseline before blaming a symbol; attributed cost (fan-out, I/O, copying, allocation, cache, locks, retries, waits); inclusive/exclusive, p50/tail.
- Efficiency: correctness, latency, throughput, and memory guardrails hold; centrality, call count, or one flame-graph sample is no proof.

- Rank by `evidence strength x consequence x actionability`; report partial scans, inferred roots, warnings, unsupported semantic tools, unmeasured claims.

## Affected scope

- Before a change, check material direct and downstream effects: consumers and contracts; stored data and migrations; resource use and failure recovery; trust boundaries; tests, generated outputs and rollout. Investigate a consequential gap rather than filling a mandatory inventory.
- On Verify, retrace for stale copies, asymmetric branches, changed defaults, widened impact.
