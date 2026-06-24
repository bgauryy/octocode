# Artifact Template — Agent Presentation Format

Standard format for presenting investigation results and findings to the user. Use this template to close the last mile between analysis and actionable communication.

---

## When to use this format

Use this template whenever you present:
- AST / LSP / search findings to the user
- investigation results from a code review, refactor, or architecture analysis
- validation verdicts (confirmed, dismissed, uncertain)
- pre-change risk assessments

Skip the template for simple answers to direct questions ("where is X defined?").

---

## Finding report template

Present each significant finding using this structure:

```
### [severity] title
**File**: path/to/file.ext (lines N–M)
**Category**: category-name
**Confidence**: confirmed | likely | uncertain

**What**: One sentence describing the problem.

**Evidence**:
1. [tool/method] → result
2. [tool/method] → result
3. [tool/method] → result

**Impact**: Who/what is affected. Blast radius (N consumers, N callers).

**Suggested fix**: Concrete next step or strategy.
```

### Example

```
### [HIGH] God function with 120 statements
**File**: src/orders/handler.ts (lines 88–210)
**Category**: god-function
**Confidence**: confirmed

**What**: processOrder orchestrates 8 side effects across 5 files in a single large function (lines 88–210 on `symbols`).

**Evidence**:
1. `read` 88–210: validation, discount, inventory, payment, email — many concerns interleaved (large/complex on inspection; no measured complexity number)
2. LSP callees: validateOrder, calcDiscount, checkInventory, chargePayment, sendConfirmation, logAudit, updateMetrics, notifyWarehouse
3. LSP callers: 3 callers (POST /orders handler, batch job, 1 test)

**Impact**: 3 production callers. Only 1 test (happy-path). High change risk — any modification touches payment, inventory, and notification flows.

**Suggested fix**: Extract pure computation helpers (validation, discount calc) first. Defer side-effect orchestration changes until transaction semantics are clarified with the team. (For a complexity number, run `eslint complexity` — ask first.)
```

---

## Summary report template

When presenting multiple findings or an overall assessment:

```
## Investigation Summary

**Scope**: what was analyzed (files, packages, features)
**Method**: which tools were used (AST patterns, LSP traces, history, any external measurement tool)

### Key Findings (N total)

| # | Severity | Category | File | Title | Confidence |
|---|----------|----------|------|-------|------------|
| 1 | HIGH | god-function | src/orders/handler.ts | processOrder: 120 statements, 8 side effects | confirmed |
| 2 | MEDIUM | dependency-cycle | src/auth/ ↔ src/session/ | Circular import between auth and session modules | confirmed |
| 3 | LOW | dead-export | src/utils/dates.ts | formatDate has 0 consumers | confirmed |

### Architecture Health
- Cycles: N
- Hotspots: top 3 files by risk score
- Critical paths: longest dependency chains

### Recommendations (priority order)
1. **[action]** — why, expected benefit
2. **[action]** — why, expected benefit
3. **[action]** — why, expected benefit

### Open Questions
- Items that need team input or are uncertain
```

---

## Validation verdict format

When validating a specific finding:

```
**Finding**: [category] description (file:line)
**Verdict**: confirmed | dismissed | uncertain
**Evidence**:
1. step → result
2. step → result
**Rationale**: Why this verdict. What would change it.
```

---

## Rules

- Always state confidence level. Never present findings as unquestioned fact.
- Always include evidence chain. At least 2 sources for `confirmed`, 1 source for `likely`.
- Always include impact. Number of consumers, callers, or affected files.
- Always include a concrete next step. Not "refactor this" but "extract X into Y, then verify with Z".
- For `uncertain` findings, state what additional evidence would resolve the uncertainty.
- Keep findings concise. If the user wants detail, they will ask.
- Group findings by severity (critical/high first) or by area (architecture, quality, security) depending on what the user asked for.
- When presenting tool output, filter and prioritize. Never dump raw match lists without triage.
- Mark any claim that rests on approximation rather than a measured metric (coupling, complexity, cycle clusters) — and name the external tool that would confirm it.
