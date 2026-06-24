# Validation Playbooks

How to validate a finding before presenting it. Every signal is a hypothesis — never present it as fact without validation. The native octocode toolset proves **shapes** (AST) and **relationships** (LSP); claims that need a **metric** (coupling, complexity, cycles-as-clusters) can't be measured natively — validate by approximation and mark them `likely`/`uncertain` (see [checklist-quality-signals.md](./checklist-quality-signals.md)).

Confidence scale: `confirmed` = ≥2 independent evidence sources agree; `likely` = one source or approximation; `uncertain` = hypothesis only. For tool commands, see [context-cli-mcp-commands.md](./context-cli-mcp-commands.md).

Shorthand as in [workflow-engineering-research.md](./workflow-engineering-research.md): `semantics <type>` = `octocode search --op <type>` / `lspGetSemantics`; `ast` = `octocode search --pattern/--rule --lang <language>` / `localSearchCode(mode:"structural")`.

---

## Category playbooks

| Category | How to validate | Typical fix |
|----------|----------------|-------------|
| Dead export | `semantics references` excluding declaration = 0, then `ast` import search = none | Remove export or wire real usage |
| Coupling hotspot | fan-in (`semantics references` count) + fan-out (`search` imports / `semantics callees`) — *approximation* | Split module by responsibility/consumer group |
| Dependency cycle | `search`/`ast` import lines both directions = present (provable natively) → hop with `semantics definition` | Break edge via shared contract / inversion |
| Security sink | `semantics callers` to trace data sources → check for guards before the sink | Add/centralize validation/sanitization before the sink |
| God function | `read` the body + `semantics callees` → count concerns and side effects | Extract focused helpers, keep orchestration thin |
| Performance (await-in-loop) | `read` the loop — is each iteration independent of N-1? | `Promise.all()` when independent; keep sequential only on real data dependency |
| Performance (sync I/O, listener leak) | `read` the body — sync I/O on a hot path? listeners without removal? | Async replacement; add cleanup |
| Test gap | `semantics references` filtered to test dirs = 0 | Add tests around the public contract and edge paths |
| Complexity / MI | `symbols` span + `read` — *no native metric* | If a number is needed, `eslint complexity` ([measurement-tools.md](./measurement-tools.md)) |

Use TDD for behavioral fixes when practical: failing test → fix → pass → full suite.

---

## Architecture interpretation (when signals are noisy)

- **Cycle:** A↔B confirmed in both directions = real, regardless of tooling. Treat overlapping cycles as one refactor unit (finding the full cluster natively is pairwise — flag it, or use `dep-cruiser`).
- **Chokepoint / broker:** high fan-in **and** high fan-out (by count) = dependency-pressure node.
- **Bridge module:** a file connecting two otherwise-separate subsystems — find it by reading what imports it from each side.
- **Package chatter:** many cross-package import lines = boundary erosion.

Prioritize fixes where high fan-in and a failure/critical path overlap.

---

## Approximated-metric cheat sheet

State these as reasoned approximations, not measurements.

| Concept | Native proxy | Read it as |
|--------|--------------|-----------|
| Instability `I = Ce/(Ca+Ce)` | `Ca` ≈ files referencing the module; `Ce` ≈ distinct imports out | a stable module depending on a more volatile one is a smell |
| Cognitive load | `symbols` span + branch/nesting keywords on read | large + deeply nested → decomposition candidate |
| Change risk | high fan-in ∩ large span ∩ recent churn (`search . --search path --modified-within`) | heuristic hotspot, not a score |

If a decision hinges on a real number, ask the user before running the external tool that produces it.

---

## Worked examples

### Example 1 — Confirmed: dead export

**Finding:** `formatDate` in `src/utils/dates.ts:42` looks unused.

| Step | Action | Result | Decision |
|------|--------|--------|----------|
| 1 | `search "formatDate"` scoped to the file | match at line 42 → real `line` | — |
| 2 | `semantics references` at line 42, exclude declaration | **0** outside the declaration | no consumers |
| 3 | broad `search "formatDate"` (file-list) | only `dates.ts` | no dynamic/re-export usage |

**Verdict: Confirmed** (high). Zero consumers in prod and test. Safe to remove.

### Example 2 — Dismissed: false-positive coupling hotspot

**Finding:** `src/config/env.ts` has high fan-in (45 references, 2 imports out).

| Step | Action | Result | Decision |
|------|--------|--------|----------|
| 1 | `read` the full file (25 lines) | exports a read-only `ENV` object from `process.env`, no logic | pure config |
| 2 | `semantics references` on `ENV` | 45 refs across 32 files, all read access, no mutation | consumers read-only |
| 3 | file discovery in `src/config --modified-within 90d` | not modified in 90 days | stable |

**Verdict: Dismissed.** High fan-in is expected and harmless for a stable read-only leaf. The count *suggested* coupling; the semantics show it isn't a refactor target — exactly the kind of approximation that must be checked, not trusted.

### Example 3 — Uncertain: god function with partial evidence

**Finding:** `processOrder` in `src/orders/handler.ts:88` is large and complex.

| Step | Action | Result | Decision |
|------|--------|--------|----------|
| 1 | `read` the body (88–210) | validation, discount, inventory, payment, email — many concerns | large on inspection |
| 2 | `semantics callers` | 3 callers: a route, a batch job, 1 test | moderate blast radius |
| 3 | `semantics callees` | 8 functions across 5 files, all side-effectful | orchestrates many effects |
| 4 | `semantics references` filtered to test dirs | 1 happy-path test | sparse coverage |

**Verdict: Uncertain** (medium). Objectively large and effect-heavy, but it may be an intentional transaction script (atomic orchestration). Cannot confirm harm without knowing whether callers expect all-or-nothing semantics and whether extraction breaks transaction boundaries. *Note: "complex" here is by inspection — no measured complexity number.*

**Recommendation:** flag for team review; if refactoring, extract pure helpers (validation, discount) first — side-effect-free and safe — and defer effect orchestration until transaction semantics are clarified.
