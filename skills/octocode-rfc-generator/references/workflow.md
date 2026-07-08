# RFC / Plan Workflow
```text
UNDERSTAND → RESEARCH (octocode-research) → PREREQUISITES? → COMPARE OPTIONS → WRITE RFC → CLOSE OPEN QUESTIONS → DERIVE KPIs → VALIDATE → DELIVER
```

Default output when saving is approved: a folder `\.octocode/rfc/{name}/` with `RFC.md`, `IMPLEMENTATION.md`, and `KPI.md`; existing-code RFCs insert `PREREQUISITES.md` before the implementation plan. See "Output shape" below for when a single `RFC.md` is enough.

## Pick mode

| User asks for | Mode | Output |
|---|---|---|
| "write RFC", "design doc", "proposal", "architecture decision" | RFC | Full RFC with alternatives, rationale, risks + implementation + KPIs |
| "plan this work", "research and build", "implementation plan" | Plan | Evidence-backed implementation plan; RFC/KPI sections included only when useful |
| "compare approaches", "should we use X or Y" | Decision | Options matrix + recommendation + adoption/rollback notes |
| "migration plan" | Migration | Current state, target state, compatibility, rollout, rollback, phases |
| "validate this RFC/design" | Validation | Claim-by-claim verdict with evidence and gaps |
| "improve/upgrade this RFC/plan", "fill the gaps", "add missing sections" | Improve | Existing doc upgraded in place: gaps closed, open questions researched, verification added |

If the task is a trivial one-file edit with no design choice, say an RFC is unnecessary and suggest using `octocode-research` Change mode directly.

## Improve an existing RFC/plan

When the input is an existing RFC/plan/design file (or a `\.octocode/rfc/{name}/` folder), upgrade it in place — don't restart. Steps:

1. Read it, then map its sections and claims against the target structure (`rfc-template.md` / `rfc-implementation.md` / `rfc-kpi.md`).
2. Diagnose gaps: missing Goals/Non-Goals, fewer than 2 alternatives, unclosed open questions, uncited or stale `file:line` claims, no verification/KPIs.
3. Use `octocode-research` to close questions and re-verify claims that may have moved.
4. Propose the upgrade (or migrate a flat `RFC-{name}.md` into the folder set), preserving the author's decisions and prior reasoning.

Gate before overwriting a saved file.

## Output shape — folder vs single file

Classify the decision first:

- **Reversible + small** ⇒ single `RFC.md` with a short inline Implementation Plan + Acceptance Criteria; say why.
- **Irreversible, cross-package, or public-contract/data/security impact** ⇒ full folder (see the SKILL.md table for each file's role):

```text
\.octocode/rfc/{name}/
  RFC.md              # decide — SSOT for goals/scope/decision; frozen at decision
  PREREQUISITES.md    # ready — existing-code RFCs only; write before implementation planning
  IMPLEMENTATION.md   # build — open questions closed via octocode-research; steps + test plan
  KPI.md              # verify — acceptance criteria + success metrics + traceability
```

**SSOT / anti-drift rule (non-negotiable):** `RFC.md` is the single source of truth for goals and scope. `IMPLEMENTATION.md` and `KPI.md` **reference `RFC.md` section anchors — they never restate goals/scope.** Generate the set in one pass from one claim ledger.

## Understand

Capture this before research gets broad:

- Problem in one or two sentences (solution-free — state the problem, not the fix).
- Why this needs a decision/plan; is the decision reversible or irreversible.
- Affected users, packages, APIs, teams, or workflows.
- Constraints: compatibility, performance, security, rollout, tech stack.
- What "do nothing" costs.
- What evidence is needed to decide, and the observable signal that would prove success.

Ask if the problem, output mode, decision flow, owner, scope, or tradeoff priority is unclear.

## Brainstorming handoff intake

If input includes an `octocode-brainstorming` RFC handoff, normalize it before research:

| Handoff field | RFC use |
|---|---|
| Problem | Motivation and affected users/workflows (`RFC.md`) |
| Chosen framing + value thesis | Summary and Rationale (`RFC.md`) |
| Surviving evidence links | Evidence Summary, Current State, References (`RFC.md`) |
| Alternatives | Alternatives Considered (`RFC.md`) |
| Constraints + risks | Drawbacks, rollout, rollback, open questions (`RFC.md` → closed in `IMPLEMENTATION.md`) |
| Bounded MVP / first slice | Implementation Plan (`IMPLEMENTATION.md`) |
| Success signal | User stories + acceptance criteria + metrics (`KPI.md`) |

For a new RFC with no handoff, ask to use available `octocode-brainstorming` first. If continuing, build the packet with `octocode-research`; ask or mark Draft when core fields are missing.

## Claim ledger

Maintain one private ledger while researching — the same ledger feeds the folder files:

```text
claim | evidence | confidence | file/section | gap / next proof
```

Use confidence `confirmed`, `likely`, or `uncertain`. Recommendations may rely only on confirmed/likely claims. Uncertain claims belong in `RFC.md` Open Questions — and each must be **closed in `IMPLEMENTATION.md` via `octocode-research`** or explicitly deferred with a reason.

## Compare options

Always include at least two alternatives (unless the user explicitly asks for a single implementation plan) — useful ones: do nothing / defer; minimal patch; incremental migration; full redesign; adopt package/library; build in-house; hybrid/phased rollout.
Compare on: fit with current architecture; blast radius; compatibility and migration cost; operational risk; performance/security/data implications; maintenance and ownership; reversibility.

## Hard gates

Stop and ask, narrow, or clearly mark `Draft` before delivery when:

- Scope contains multiple independent decisions → split into RFCs or phases.
- Flow uncertainty would change the artifact shape or recommendation → ask before drafting.
- Current state has no real evidence → research more or mark as unproven.
- Only one option is considered and the user did not ask for a single-path plan → add do-nothing, minimal patch, or phased rollout.
- Public API/data/security/compatibility changes lack rollout, rollback trigger, and owner/approver.
- An `RFC.md` open question is still `uncertain` and unclosed in `IMPLEMENTATION.md`.
- Brainstorming handoff says `not ready`, `Prototype First`, `Narrow`, `Park`, or `Do Not Build`.

## Write the files

Produce the files in dependency order — `RFC.md` first (the SSOT), then `PREREQUISITES.md` for existing-code RFCs, then `IMPLEMENTATION.md`, then `KPI.md` — each from its own template:

- `RFC.md` — see `references/rfc-template.md`.
- `PREREQUISITES.md` — see `references/rfc-prerequisites.md`; write it before the plan and cite current-state facts, blockers, owners, baseline checks, setup, and contract/migration constraints.
- `IMPLEMENTATION.md` — see `references/rfc-implementation.md`. Opens by **closing every `RFC.md` open question with an `octocode-research` citation**, then dependency-ordered steps + V&V + rollback. References `RFC.md` anchors; no restated goals.
- `KPI.md` — see `references/rfc-kpi.md`. User stories, Gherkin criteria, metrics (never a single one), a decision rule, and the traceability matrix.

For a lighter **Plan** in single-file mode, keep only the `rfc-template.md` sections that carry weight — typically Goal, Current State, Proposed Approach, Alternatives, Step-by-Step, Test/Verification, Rollout/Rollback, plus inline Acceptance Criteria and Open Questions.
Order steps by dependency, not preference; avoid time estimates.

## Validate

Before delivering, check:

- Problem, motivation, Goals **and** Non-Goals are specific; current state has real evidence.
- Alternatives fairly compared, including do-nothing; recommendation depends on no `uncertain` claim.
- Drawbacks, pre-mortem (how this fails), migration cost, and blast radius (shared symbols/contracts) are explicit.
- **Every `RFC.md` open question is closed in `IMPLEMENTATION.md` with a citation, or deferred with a reason.**
- Implementation plan is actionable, dependency-ordered, and verifiable.
- `KPI.md` has user stories, testable (Gherkin) acceptance criteria, a primary success metric with baseline/target/window, guardrails, a decision rule, and a traceability matrix covering every RFC requirement.
- SSOT holds: `IMPLEMENTATION.md`/`KPI.md` reference `RFC.md` anchors and do not restate goals/scope.
- Every non-obvious claim has a citation or is marked uncertain; no claim relies on "common practice" without explaining why it applies here.

Reasoning traps: **first-option bias** (search for evidence against the preferred approach); **false dichotomy** (consider hybrids/phased plans); **local-vs-external conflict** (local constraints usually win — document the tradeoff); **metrics claims** (use external tools or mark as approximation).

## Deliver

Start with a concise summary:

```text
Status: <Draft|Ready for Review|Blocked>
Decision: <recommendation>   (reversible | irreversible)
Why: <1-2 evidence-backed reasons>
Alternatives: <count and names>
Risk: <low|medium|high + why>
Success signal: <the KPI that proves it worked>
Next step: <one action>
```

Then ask whether to save.

- **Save (full):** create `\.octocode/rfc/{name}/` with the selected document set. **Save (single):** `\.octocode/rfc/{name}/RFC.md` (or flat `\.octocode/rfc/RFC-{name}.md` on request).
- **No:** keep it in chat. **Implement:** hand off to the agent's normal engineering workflow using `IMPLEMENTATION.md`; verify against `KPI.md` after shipping.
