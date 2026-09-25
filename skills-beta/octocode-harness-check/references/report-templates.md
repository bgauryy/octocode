# Report templates

Load when writing REVIEW.md or PLAN.md.

## REVIEW.md

```markdown
# Octocode harness check — <date>
Mode: diagnostic audit (not a KEEP/ACCEPT verdict).
Inputs: orangu <version> over <n> sessions; probe <cases>×<surfaces>; binaries <paths + mtimes>; contract fingerprint <hash>.
Caveats: <drift override, offline, missing token, …>

| Axis | Result |
|---|---|
| Contract / crash | … |
| Content checks | pass/total per surface |
| CLI parity | … |
| MCP vs CLI payload | median/max ratio; structuredContent duplication |
| Latency | cold CLI vs warm MCP; LSP |
| Catalog weight | tools, chars, ≈tokens; heaviest schemas |
| History | per-tool calls/errors; triaged error classes |

## Findings (ranked P1–P4)
### P<n> — <claim>
Evidence: <probe case + output excerpt | file:line>. Impact: <who pays, how>. Fix: <owner surface + change>.

## What is solid
## Reproduce
```

## PLAN.md

```markdown
KPI: each finding closes with a failing-before / passing-after check.
Guardrails: 0 contract violations, 0 crashes, every next.* runnable verbatim, no content check regresses.

| # | Finding | Owner surface | Change | Verify |
|---|---|---|---|---|

Order: parallel work on disjoint files → one core regen → one native rebuild → probe rerun → compare-runs.
```

Owner surfaces: **core** (schemas, descriptions, instructions) · **native** (validation, execution, output shape, continuations) · **interface** (MCP/CLI rendering and error text) · **docs/config**.
