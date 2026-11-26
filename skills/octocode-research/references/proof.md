Load when a consequential finding (cycle, affected scope, security sink, test gap, performance) or a root cause needs its proof bar.

# Proof and root cause

## Prove

Ladder: `candidate → exact evidence → claim-specific corroboration → verification → verdict`.

| Finding | Minimum corroboration |
|---|---|
| cycle | graph `cycles` + exact imports; type-only vs runtime |
| affected scope | graph `impact`/`dependents`/`path` + exact reads + LSP references/callers |
| security sink | sink shape + exact read + sources/callers + guard check |
| test gap | changed symbol + no test references + nearby test read |
| performance | exact hot path; benchmark only when runtime proof matters |

- Keep syntax, semantic, history, artifact, and runtime proof separate; list checks not run.

## Root cause

Apply the [research loop](../SKILL.md#research-loop); a suspicious line, commit, or correlation stays a candidate until all four hold:

1. Expected vs actual, and the contract behind it.
2. Reproduce (or equivalent runtime evidence); first divergent boundary.
3. Rivals, each with its most distinguishing check: source, references, history, tests.
4. Changing the cause changes the symptom; if it cannot run, state equivalent evidence and limits.
