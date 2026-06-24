# Quality Signal Catalog

Quality signals to look for, and how to get evidence for each with the **native octocode toolset** — or, where the native tools can't *measure*, the approximation and the external tool that can. This skill ships no scanner; treat every signal as a hypothesis and prove it before presenting (see [workflow-validation-playbooks.md](./workflow-validation-playbooks.md)).

Three evidence classes:

- **Shape** — provable with AST (`octocode search --pattern/--rule --lang <language>` / `localSearchCode mode:"structural"`). Near-zero false positives.
- **Relationship** — provable with LSP (`references`, `callers`/`callees`) + import reads.
- **Measurement** — a number (complexity, MI, coupling, type-coverage %, cycles). **Not native.** Approximate, or run an external tool from [measurement-tools.md](./measurement-tools.md) and say which.

---

## Shape signals (AST — native, authoritative)

Copy the patterns from [context-ast-pattern-cookbook.md](./context-ast-pattern-cookbook.md).

| Signal | Pattern intent | Why it matters |
|--------|----------------|----------------|
| Empty catch | `catch_clause` with no handling body | silently swallows errors |
| Catch-only-rethrow | `catch` whose only statement is `throw` | dead try/catch or missing context |
| `console.*` / `debugger` / `print()` left in | call/statement match | debug leakage in prod |
| Explicit `any` / `as X` / `x!` | kind/pattern match | type escape at a boundary |
| `eval` / `exec` / dynamic code | call match | injection sink |
| `import * as X` / `from X import *` | import match | unclear surface, dead-code blindspot |
| Nested ternary, deep callbacks | shape match | readability |
| `Promise.all` without handling | call match | unhandled rejection |
| `await` in a loop | `--rule` `await inside for…` | serial where parallel is possible |
| Sync I/O (`readFileSync`, …) | call match | blocks the event loop on a hot path |
| Bare/broad `except`, mutable default arg (Py) | kind match | silent failure / shared-state bug |
| Magic string/number repeated | literal match + count by reading | drift risk; extract a constant |
| Throwing a string | `throw` of a string literal | loses stack/type |

---

## Relationship signals (LSP — native)

| Signal | How | Verdict |
|--------|-----|---------|
| Dead export | Smart OQL `target:"research"` candidate + `references` excluding declaration → 0 + `ast` import search → none | confirmed dead only after candidate row and semantic/structural proof agree |
| Blast radius | `references` (split test vs prod) + `callers` | sizes the change |
| Coupling — fan-in | count of `references` into a module | high fan-in = depended-on |
| Coupling — fan-out | count of imports out (`search` import lines) | high fan-out = depends-on-many |
| God function (orchestration) | `callees` → count distinct side-effect calls | many concerns = extract candidate |
| Cohesion | which exports are used together by which consumers (`references` per export) | scattered usage = split candidate |
| Layer/dependency-rule violation | `references`/`definition` direction across packages | inward-only is correct |

---

## Measurement signals (NOT native — approximate or use an external tool)

These were computed by the old embedded scanner. The native CLI/MCP do not produce them. **Flag any claim that rests on these as `likely`/`uncertain` and name the tool that would confirm it.**

| Signal | Native approximation | Measure it with |
|--------|----------------------|-----------------|
| Cyclomatic / cognitive complexity | read the function; eyeball branches/nesting | `eslint --rule '{"complexity":["error",10]}'` |
| Maintainability Index, Halstead effort | — (no native proxy) | dedicated metrics tooling; treat as advisory |
| Dependency cycles (SCC clusters) | `search` import lines both directions + LSP hop the chain | `dep-cruiser --no-config -T err` |
| Instability `I = Ce/(Ca+Ce)`, SDP | compute by hand from fan-in/fan-out counts | `dep-cruiser` |
| Chokepoints / articulation points | high fan-in ∩ high fan-out by inspection | `dep-cruiser` graph output |
| Type-safety % | per-file `any` via `ast` | `type-coverage --strict --detail` |
| Dead code (framework-aware) | Smart OQL `target:"research"` + LSP `references` + `ast` imports | `knip` |
| Near-clone density | sample with `ast` shape patterns | a clone-detector external |
| Change-risk hotspot | high fan-in ∩ large span (`symbols`) ∩ churn (`search . --search path --modified-within`) | composite — state it's heuristic |

### Reasoning from approximations (when you can't run a tool)

- **Coupling:** `Ca` ≈ number of files that `references` a module's exports; `Ce` ≈ number of distinct modules it imports. High `Ca` + low `Ce` = a stable leaf (often fine). High both = a hub under pressure.
- **Complexity:** a function whose `symbols` span is large and whose body has many guard/branch keywords on read is a decomposition candidate — say "large/complex on inspection," not "complexity = N."
- **Cycles:** if A imports B and B imports A (confirm both directions with `search`/`ast` on import lines), it's a cycle regardless of tooling — that one is provable natively.

---

## Severity & confidence

| Severity | Use for |
|----------|---------|
| critical | data loss, security sink with reachable untrusted input, broken public contract |
| high | god function on a hot path, confirmed cycle across core modules, swallowed error in a failure path |
| medium | local complexity/duplication, type escape at a boundary, missing cleanup |
| low | style, isolated smell, advisory |

Pair severity with a confidence level: `confirmed` (evidence from ≥2 independent tools/reads), `likely` (single source or approximation), `uncertain` (hypothesis not yet verified). A measurement-class claim without a tool maxes out at `likely`.
