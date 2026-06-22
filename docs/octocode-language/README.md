# Octocode Language Docs

This folder is the planning and implementation guide for Octocode Query
Language.

## Read Order

| Doc | Use it for |
|---|---|
| https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE.md | XML-tagged canonical OQL contract and examples |
| https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OQL_RESEARCH_GRAPH_FLOW.md | AST/LSP-first research graph flow, language tiers, Rust/tool-core split, and dead-code proof contract |
| https://github.com/bgauryy/octocode/blob/main/docs/octocode-language/OCTOCODE_QUERY_LANGUAGE_PLAN.md | Implementation plan, prerequisites, package split, milestones, tests, and risks |

## One-page Decision

OQL is a typed query object that compiles to existing Octocode capabilities. It
is not a new raw DSL. The contract is Markdown with XML-style tags so agents
can chunk the prompt into stable instruction blocks. It has one canonical shape:
`target`, `from`, `scope`, discriminated `where.kind`, `materialize`, `fetch`,
`select`, `view`, `controls`, result bounds, diagnostics, provenance,
evidence, and executable `next.*` continuations. It also defines a bounded
batch envelope for 1-5 independent queries, plus `target:"graph"` for
agent-readable relationship nodes, edges, facts, packets, and missing proof.

Command split:

- `octocode search`: universal OQL runner for research targets.
- Existing quick commands (`grep`, `cat`, `ls`, `find`, `lsp`, `repo`, `pkg`,
  `pr`, `history`, `binary`, `diff`, `clone`, `cache`) remain available until
  parity gates prove `search` can replace each workflow.
- Raw `tools NAME` calls remain the schema-exact compatibility and debug
  surface.

Implementation split (as shipped):

- OQL schema, types, normalizer, planner, adapters, and result envelope live in
  `packages/octocode-tools-core/src/oql` (co-located for now; may migrate to a
  `@octocodeai/octocode-core/oql` export once a second consumer needs OQL
  validation without the rest of tools-core).
- Public descriptions / command text live in `@octocodeai/octocode-core`.
- Native primitives stay in `packages/octocode-engine`.
- CLI (`octocode search`) and MCP stay thin wrappers over `runOqlSearch`.

Research-graph split:

- Structure/files bound the corpus before semantic claims.
- Ripgrep and file predicates are discovery only, never deletion proof.
- AST and LSP are the proof foundation.
- `target:"research"` returns packet candidates; `target:"graph"` projects
  those packets into relationship queries such as "what keeps this alive?".
- Rust should parse files, extract normalized AST facts, connect graph
  nodes/edges, and run deterministic graph algorithms.
- tools-core/OQL should own research intent, framework/package entrypoint
  policy, LSP proof escalation, and agent-facing packets.

## Implementation Checklist

1. Add strict OQL schema types.
2. Build the normalizer: sugar in, canonical OQL out.
3. Build planner with predicate-node IDs and `PUSHDOWN`, `RESIDUAL`, `ROUTE`,
   and `UNSUPPORTED`.
4. Adapt canonical OQL to current local and GitHub tools.
5. Promote bounded remote-as-local from CLI behavior into tools-core.
6. Standardize result envelope: `results`, `pagination`, executable `next`,
   `diagnostics`, `provenance`, and `evidence`.
7. Add `--explain` with normalized query, per-predicate routing, defaults,
   budgets, backend calls, materialization, diagnostics, and continuations.
8. Wire CLI and MCP without duplicating logic.
9. Use the OQL contract and golden parity tests as the gate before replacing any
   quick command or raw tool path.

## Editing Rules

- Keep this folder short and implementation-facing.
- Do not duplicate the full target contract in the plan.
- Do not put current implementation details in the target contract unless they
  define an intentional compatibility bridge.
- Keep the OQL contract XML tags balanced and meaningful for agent attention.
- Use absolute GitHub URLs for documentation links.
