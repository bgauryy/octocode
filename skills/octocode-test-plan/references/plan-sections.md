Load when writing the Answer, Scope, Features, Success criteria, Not tested, Findings or Gaps section of a test plan. [output.md](../output.md#saved-report) owns the section order.

# Plan sections

## Answer

Readiness, main risks and the gap most likely to change the result.

## Scope

Repository, revision (SHA or PR head), paths/features and source anchors. Identify relevant working-tree and untracked changes when they affect the planned result.

## Features

Map every in-scope feature, documented claim or changed behavior:

| Feature or claim | Source | Risk | Priority | Cases |
|---|---|---|---|---|
| `<behavior>` | `<path:line or spec>` | `<what could fail and why>` | `P0/P1/P2` | `<case IDs or Not tested>` |

## Success criteria

Set measurable run criteria. Default, adapt explicitly:

- Every P0 and existing regression case passes.
- Report failed or blocked cases with their impact; any acceptance exception needs an owner decision.
- Every case has evidence; missing evidence means `blocked`.
- Source/tests remain unchanged: the starting-state comparison exits 0 with no output.

When the project enforces a coverage floor or the change carries a performance bound, add it with baseline and units.

## Not tested

Each excluded feature, risk or environment, with its reason or a named coverage owner.

## Findings

Optional, before Gaps: defects encountered while mapping, with deciding anchors and a proposed proving case. Planning does not repair them.

## Gaps

Unread scope, unavailable semantic/graph evidence, unconfirmed commands, contradictory docs/code with both anchors when nothing decides which side is right (a decided contradiction is a [Finding](#findings)), and labeled assumptions.
