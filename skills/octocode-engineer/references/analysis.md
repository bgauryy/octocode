Load when modeling a design, boundary, interface, or flow, or when topology, cycles, reachability, dead code, duplication, coupling, or efficiency affects a decision.

Read next: [graph evidence and controls](analysis-topology.md) when a graph result or structural signal is the lead; [claim limits, proof and ranking](analysis-proof.md) when stating what a finding proves or ranking findings.

# Analysis

## Model the wiring

- Name each hop's interface ([contract trace](checks.md#contract-trace)); make invalid states unrepresentable; keep core decisions apart from UI, storage, transport.
- Record `module/layer -> responsibility -> owned data/invariants -> public interface -> allowed dependencies`; name each role and style with [concepts](concepts.md#use-a-concept-in-a-review).
- Cross-check views; each hides what another exposes: static (file/symbol edges), control (who initiates, retries, cancels, handles errors), data ([data-flow trace](checks.md#data-flow-trace)), ownership (invariants, lifecycles, contracts), runtime (config, registries, processes, generated artifacts, dynamic dispatch).
- For a bounded decision, rejoin views for representative scenarios as `trigger -> decision -> transformation -> side effect -> result/error`; note unresolved reflection, queues, registration, external systems, trust and transaction boundaries. For a whole-system audit, cover every inventoried surface and junction rather than sampling flows; `octocode-workflows`, when installed, provides a traversal procedure.
- Name the quality attribute judged (correctness, security, changeability, performance, operability); [kill the strongest alternate](analysis-topology.md#kill-the-strongest-alternate) for look-alike signals.
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

## Affected scope

- Before a change, check material direct and downstream effects: consumers and contracts; stored data and migrations; resource use and failure recovery; trust boundaries; tests, generated outputs and rollout. Investigate a consequential gap rather than filling a mandatory inventory.
- On Verify, retrace for stale copies, asymmetric branches, changed defaults, widened impact.
