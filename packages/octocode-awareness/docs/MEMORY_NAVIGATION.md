# Attend And Active Memory Navigation

**Audience**: maintainers and agents evaluating the next awareness planning surface.

`attend` is the shipped read-only helper for choosing which existing awareness surfaces to inspect before work. It gives agents a compact start packet with repo profile, workboard rows, selected memory evidence, gaps, verification targets, bloat warnings, organ state, and drive state.

The older `memory navigate` idea remains a possible deeper trace feature, but agents should start with:

```bash
octocode-awareness attend --workspace "$PWD" --query "current task" --compact
octocode-awareness query workboard --workspace "$PWD" --format table --limit 20
```

Then use the explicit workflow in `SKILL.md` and `docs/SKILLS.md`: `workspace status`, `memory recall`, `refinement get`, `signal list`, `query <view>`, and `reflect mine-weakness` as needed.

## Context Circulation

Context and tokens are like circulation for an agent run. They carry useful oxygen: goal, constraints, live risks, cited lessons, handoffs, and next verification targets. A compact packet keeps that circulation moving.

Oversized docs are like excess weight. They may contain useful stored energy, but if every run has to carry them, the system gets slower and less agile. The navigation rule is: start compact, route to rows, and only open long docs when the trace says they are needed.

Sociality is part of intelligence here. Signals, refinements, and handoffs let other agents and humans add new perspectives. Keep those traces concise and resolvable so they feed the shared map instead of becoming stale mass.

## Problem

Awareness already has the data an agent needs, but the starting checklist can branch quickly:

- current locks and pending verification live under `workspace status` and `verify audit`,
- reusable lessons live under `memory recall` and `query memories`,
- repo gotchas and decisions live under `query gotchas` and `query lessons`,
- active handoffs live under `refinement get` and `signal list`,
- repeated failure patterns live under `reflect mine-weakness`.

`attend` makes that routing explicit without inventing a new memory store.

## Shipped Command

The shipped shape is:

```bash
octocode-awareness attend \
  --workspace "$PWD" \
  --query "current task" \
  --compact
```

It returns a deterministic trace showing which existing reads it chose, why evidence was selected, which verification gaps remain, and which workboard rows need attention.

## MVP Boundary

The first version is read-only and deterministic.

Inputs:

- `--workspace`, optional `--artifact`, `--repo`, and `--ref`,
- a natural-language `--query`,
- optional labels, tags, files, and limits that map to existing recall/query filters.

Outputs:

- `trace`: ordered steps and counts,
- `evidence`: compact references to memories, refinements, signals, locks, or query rows,
- `gaps`: missing or low-confidence areas that need live verification,
- `next_verification_targets`: files, commands, or docs the agent should inspect before relying on the result.

Non-goals:

- no autonomous edits,
- no replacement for `memory recall`,
- no embedding requirement,
- no hidden policy engine,
- no changes to the canonical SQLite schema unless trace fixtures prove the existing views are insufficient.

## Candidate Routing

```text
workspace status
  -> active locks or pending verification?
       yes: surface coordination and verify/audit next steps
       no: continue
memory recall --smart
  -> enough high-confidence evidence?
       yes: return evidence plus validation targets
       no: query gotchas/lessons/files/activity
refinement get + signal list
  -> unfinished state or messages?
       yes: include handoff or inbox actions
reflect mine-weakness
  -> repeated failure signature relevant to query?
       yes: include weakness evidence and caution
```

## Remaining Trace Fixture Requirement

The shipped `attend` command has fixture coverage for the first slice. Add deeper fixtures before expanding it into richer navigation:

- clean workspace with no relevant memory,
- active file lock conflict,
- pending verification from a previous edit,
- stale memory superseded by a newer fact,
- handoff refinement plus unread signal,
- repeated failure signature that should change the plan.

These fixtures should test the returned trace, not only result counts. The point of the feature is explainable routing with bounded context.

## Relationship To Existing Features

Active memory navigation composes existing features:

| Existing surface | Role in navigation |
|---|---|
| `attend` | Compact start packet and routing trace. |
| `query workboard` | Row/column view for active work, verification debt, memory review, and projection health. |
| `workspace status` | Operational starting state. |
| `memory recall` | Main reusable lesson search. |
| `query <view>` | Structured repo, task, lock, signal, and activity inspection. |
| `refinement get` | Unfinished work and handoffs. |
| `signal list` | Live messages and coordination. |
| `reflect mine-weakness` | Repeated failure patterns. |

The SQLite DB remains canonical, and generated `.octocode/` wiki files remain projections. Current source, tests, and user instructions still beat remembered context.
