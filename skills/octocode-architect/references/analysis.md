# Analysis

Load when modeling a design, boundary, interface, or flow, or when topology, cycles, reachability, dead code, duplication, coupling, or efficiency affects a decision.

## Model the wiring

- Name each hop's interface ([contract trace](checks.md#contract-trace)); make invalid states unrepresentable; keep core decisions apart from UI, storage, transport.
- Record `module/layer -> responsibility -> owned data/invariants -> public interface -> allowed dependencies`.
- Cross-check views; each hides what another exposes: static (file/symbol edges), control (who initiates, retries, cancels, handles errors), data ([data-flow trace](checks.md#data-flow-trace)), ownership (invariants, lifecycles, contracts), runtime (config, registries, processes, generated artifacts, dynamic dispatch).
- For a bounded decision, rejoin views for representative scenarios as `trigger -> decision -> transformation -> side effect -> result/error`; note unresolved reflection, queues, registration, external systems, trust and transaction boundaries. For a whole-system audit, follow [single-actor traversal](../../octocode-workflows/references/single-actor-traversal.md) and cover every inventoried surface and junction rather than sampling flows.
- Name the quality attribute judged (correctness, security, changeability, performance, operability); [kill the strongest alternate](#kill-the-strongest-alternate) for look-alike signals.
- Enforce layering in the build (`cargo deny` `[bans]` for Rust, a boundary test for TypeScript); a prose-only rule is `inferred`.
- Generated source of truth (contract, schema, rule table): assert each hand-copied string (messages, allowlists, defaults) exists in the generated source; outcome parity tests miss copied text.

## Decompose large tasks

- Split into the smallest verifiable problems, each a Task contract: objective, inputs, constraints, output, Definition of Done (the verification point); order by dependency edges.
- Parallelize only with clear ownership and a latency or independent-evidence gain, never to hide failures.
- Compose after parts pass; test shared interfaces end to end.

## Explore the graph and find critical paths

- Seed exploration from exposed tools, user flows, protocol ingress, state owners, lifecycle hooks, and observed failures or profiles. Reconcile the graph with runtime registration; review the whole requested inventory, not only the graph's highest-ranked issues.
- Expand breadth through callers, dependents, dependencies, implementations, and consumers. For each junction, ask what enters, what branches, what state changes, and who consumes the result. Keep unresolved dynamic or external edges visible.
- Follow depth from a concrete trigger to its effect and observable outcome, then backtrack through error, fallback, retry, cancellation, and cleanup branches. Include asynchronous callbacks, queues, and fan-out/fan-in; an import chain alone is not an execution path.
- Use the workflow [AST technique](../../octocode-workflows/references/ast-lsp-workflows.md#ast-workflow) to locate structures and its [LSP technique](../../octocode-workflows/references/ast-lsp-workflows.md#lsp-workflow) to resolve decisive identities. Preserve a frontier of unexplored edges and deduplicate by source identity and relevant scenario; record cycles without walking them forever.
- Distinguish correctness-critical paths from latency-critical paths. A rarely used recovery or authorization path can be critical; high fan-in is only a lead for a runtime hotspot. Establish the workload and baseline before ranking measured cost.
- At each critical path, apply the [review angles](checks.md#review-angles), record healthy invariants and disproved suspicions, and identify the next check that could change the verdict. Trace a changed boundary's upstream and downstream effects before closing it.

## Choose the narrowest evidence lane

Topology triages file relationships; it never proves symbol identity (use semantic references), data/control flow, runtime behavior, or impact.

| Question | Start with | Candidate for |
|---|---|---|
| What a file affects or relies on | `graph query dependents`/`deps <file> [--depth n]`; `graph query impact <ref>` (`impact --since <rev>` for a diff) | Blast radius, coupling |
| Layer reachability | `graph query path <from> <to>`, plus the declared rule | Boundary crossing |
| Mutual dependency | `graph query cycles`; `issues --detector cycle` (runtime imports only, witness and `suggestedCuts`) | Initialization or change coupling |
| Unreachable or removable | `graph query issues --detector unreachable-file,unused-export,test-only` with explicit `--entry <ref,...>` | Alternate entrypoint, stale code |
| New edges or cycles | `graph query drift --graph <head> --baseline <baseline>` (ingest both trees), or `issues --baseline <snapshot>` | Drift |
| Review order | `graph query issues` (ranked hypotheses with `controls` and `verify` commands) | Hotspot, never a finding |

- Not topology: duplicated policy or leaky interfaces (scenarios, consumers, AST/LSP, history); runtime cost (profile, end-to-end benchmark); slow builds (timed clean/warm runs, I/O, process, cache counts).
- Rust: crate edges from `cargo metadata --format-version 1` (or `cargo tree -e normal`); `rust-best-practices` when installed.
- Run `npx octocode graph ingest <path>` once (an unchanged tree reuses its snapshot), then `npx octocode graph query <op>`; re-ingest when `graph query stale` lists changed files.
- Reruns: fix the ingested root, `--entry` entrypoints, and detectors.
- Run each `issues` finding's `verify` commands unchanged.
- Before an absence or completeness claim, check consumers outside the scanned roots and coverage signals: `graph` `graph.partialReasons` (run its `graph.reingest` command), `callInternalRecall`, finding `tier`, and `unresolved-import` findings; `lspSearch` `coverage.exhaustive`. Cold LSP servers undercount references: rerun, or treat partial as a lower bound.

## Kill the strongest alternate

| Signal | Common false explanation | Required control |
|---|---|---|
| Cycle | Type-only closing edge; intentionally cohesive module | Exact-read every edge; check `edgeKinds` and runtime impact; prove implicit edges (`java-same-package`, no import line) with `lspSearch` references |
| Unreachable/dead | Subpath export, CLI/plugin/framework registration, dynamic import, tests or consumers outside the root | Inspect manifests, configs, broader consumers; verify symbols separately |
| Barrel-only export | External or deep-import public contract | Package surface and external consumers (`dependents` rows with `reexportVia`, file-grouped `lspSearch` references) |
| Cross-folder edge | Folder names imply layers without a rule | Find an authoritative rule or label the boundary inferred |
| Similar code | Distinct rules, trust boundaries, volatility | Compare semantics, edge cases, ownership, change pressure |
| Wide node or long path | Composition root, generated registry, stable shared value, cold path | Inspect responsibilities; measure the claimed cost |
| Small interface | Unsafe call sequence or leaked lifecycle | Consumer workflows, invariants, errors, cancellation, versioning |

- Each control is a [hypothesis test](../SKILL.md#steps); an alternate left unresolved keeps the finding a candidate.

## Proof and ranking

- Prove in order: `graph candidate -> exact decisive edges/code -> AST shape -> LSP definitions/references/callers -> representative data/control/ownership trace -> config/test/runtime/measurement -> verdict`.

- Affected scope: graph dependents/path plus semantic references to the changed symbol.
- Layer violation: exact path, importing symbol, authoritative rule, quality impact.
- Dead code / delete: explicit roots, export/re-export chain, semantic references excluding declarations, registration/config search, then tests/build.
- Coupling or god module: graph breadth plus mixed responsibilities and real consumer or change impact, not size.
- Duplication: semantic equivalence, shared invariant, shared change pressure, not text similarity.
- Separation/interface: inward framework/storage/view dependency, invariant owner, consumer contract, policy test seam.
- Hot path: named workload and budget; end-to-end baseline before blaming a symbol; attributed cost (fan-out, I/O, copying, allocation, cache, locks, retries, waits); inclusive/exclusive, p50/tail.
- Efficiency: correctness, latency, throughput, and memory guardrails hold; centrality, call count, or one flame-graph sample is no proof.

- Rank by `evidence strength x consequence x actionability`; report partial scans, inferred roots, warnings, unsupported semantic tools, unmeasured claims.

## Affected scope

- Before a change, map direct and second-order effects on: interfaces, callers, dependency edges; persisted data, schemas, defaults, migrations; latency, memory, retries, concurrency, failure containment; trust boundaries, sensitive data, authorization, untrusted or model output; tests, telemetry, rollout, rollback, generated outputs, records. Material ones only; a surprising omission gets N/A with a reason.
- On Verify, retrace for stale copies, asymmetric branches, changed defaults, widened impact.
