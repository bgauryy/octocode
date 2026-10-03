# Quality and output

Load when you judge, rank, or present skill candidates, or deep-dive a candidate. Why: consistent dimensions and cards make candidates comparable.

Read enough `SKILL.md` to understand behavior. For strong, risky, or ambiguous candidates, read the full `SKILL.md` plus the scripts, templates, install docs, and references that affect execution.

## Content dimensions

| Dimension | Check |
|---|---|
| Trigger | activation and non-activation boundaries |
| Workflow | ordered steps, decisions, recovery, stop conditions |
| Evidence | real files, resources, tests, examples, scripts |
| Gates | validation, approval, preview, permissions, rollback, conflicts |
| Output UX | concise results, comparison cards, next-step gate |
| Specificity | domain knowledge the agent lacks by default |
| Portability | runtime assumptions, hardcoded paths, dependencies, secrets |
| Risk | unsafe commands, hidden network, missing references, license, stale docs, broad triggers |

Labels: `High` = direct match, clear trigger, executable workflow, useful gates, no red flags. `Medium` = partial or adaptable; some validation, UX, or domain detail is missing. `Low` = keyword-only, generic, unclear trigger, stale, or a real caveat.

## Adoption signals (tiebreakers)

| Signal | Source |
|---|---|
| Install count | `skills.sh/api/search?q=` sorted by `installs`; high installs with modest stars usually beat the reverse |
| Per-skill page | `skills.sh/<owner>/<repo>/<skill>`: installs, install command, audit badge, siblings |
| Recency | GitHub `pushed:>YYYY-MM-DD`; skip >12 months stale unless archival is wanted |
| Audit badges | skills.sh Gen Agent Trust Hub; Microsoft Sensei (triggers, anti-triggers, compatibility) |
| Registry fields | aiskillstore `match_reasons`, `downloads_7d`, `days_since_update` |
| Overlap and demand | `aiskillstore.io/v1/agent/skills/{id}/similar`; `aiskillstore.io/v1/demand/most-wanted` (adapt vs create) |

## Present

Group only when useful: Best matches / Useful alternatives / Explore if…. With many results, list names and sources; detail only the strongest.

```text
Name:            <skill> — fit: High | Medium | Low
Source:          <owner/repo/path> or <local path>
What it does:    <one sentence>
Actual flow:     <2-4 steps from inspected content>
Quality signals: <specific evidence>
Why it matches:  <tie to request>
Caveat:          <real risk, or "None obvious">
```

## Deep-dive

Summarize trigger, workflow, support files, gates, strengths, gaps, and adaptation ideas. Offer only relevant next actions.

Next: when installing or adapting load `references/install.md`; if evidence is thin or a surface fails load `references/recovery.md`.
