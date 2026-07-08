# Active Memory Navigation

**Audience**: maintainers and agents evaluating the next awareness planning surface.

Active memory navigation is a proposed read-only helper for choosing which existing awareness surfaces to inspect before work. It is not a shipped command yet. Agents should keep using the explicit workflow in `SKILL.md` and `docs/SKILLS.md`: `workspace status`, `memory recall`, `refinement get`, `signal list`, `query <view>`, and `reflect mine-weakness` as needed.

## Problem

Awareness already has the data an agent needs, but the starting checklist can branch quickly:

- current locks and pending verification live under `workspace status` and `verify audit`,
- reusable lessons live under `memory recall` and `query memories`,
- repo gotchas and decisions live under `query gotchas` and `query lessons`,
- active handoffs live under `refinement get` and `signal list`,
- repeated failure patterns live under `reflect mine-weakness`.

The proposed helper should make that routing explicit without inventing a new memory store.

## Proposed Command

The candidate shape is:

```bash
octocode-awareness memory navigate \
  --workspace "$PWD" \
  --query "current task" \
  --compact
```

It should return a deterministic `navigation_trace` showing which existing reads it chose, why they were chosen, which evidence was found, and which verification gaps remain.

## MVP Boundary

The first version should be read-only and deterministic.

Inputs:

- `--workspace`, optional `--artifact`, `--repo`, and `--ref`,
- a natural-language `--query`,
- optional labels, tags, files, and limits that map to existing recall/query filters.

Outputs:

- `navigation_trace`: ordered steps and reasons,
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

## Trace Fixture Requirement

Before shipping the command, add fixtures that assert the trace for common scenarios:

- clean workspace with no relevant memory,
- active file lock conflict,
- pending verification from a previous edit,
- stale memory superseded by a newer fact,
- handoff refinement plus unread signal,
- repeated failure signature that should change the plan.

These fixtures should test the returned trace, not only result counts. The point of the feature is explainable routing.

## Relationship To Existing Features

Active memory navigation composes existing features:

| Existing surface | Role in navigation |
|---|---|
| `workspace status` | Operational starting state. |
| `memory recall` | Main reusable lesson search. |
| `query <view>` | Structured repo, task, lock, signal, and activity inspection. |
| `refinement get` | Unfinished work and handoffs. |
| `signal list` | Live messages and coordination. |
| `reflect mine-weakness` | Repeated failure patterns. |

The SQLite DB remains canonical, and generated `.octocode/` wiki files remain projections. Current source, tests, and user instructions still beat remembered context.
