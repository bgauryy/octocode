---
name: octocode-clean-agentic-code
description: "Use when cleaning up existing code without changing behavior: proven dead code, unused exports, duplicate helpers, temporary logs and agent leftovers. Check callers and contracts before removal; report weakened tests or masked defects rather than hiding failures."
---

# Octocode clean agentic code

For each candidate in the authorized scope, identify the behavior removal could break and the evidence that would reveal it. Check callers, public entrypoints, config, dynamic loading and external consumers. Use [Octocode discovery and exact reads](references/octocode-usage.md) or `octocode-research` to trace effects. Confirm symbol identity with semantic references. Graph edges, file size, pattern matches and empty searches alone cannot prove redundancy.

| Candidate | Decision |
|---|---|
| [Dead or duplicate code](references/code-smells.md) | Remove proven redundancy in reviewable batches. Keep one owner, update callers and links, and regenerate derived files and lockfiles with their generator. Keep live code. Defer unresolved cases with the check that would settle them. |
| [Comments, instructions or configuration](references/text-config-hygiene.md) | Keep text and configuration with a parser, runtime or external consumer until its contract deliberately changes. If cleaner wording changes agent behavior, use `octocode-agentic`. For skill packaging or activation, use `octocode-skills`. |
| [Agent leftovers or defects](references/agent-defects.md) | Report error masking and reachable stubs as [SBAR blockers](references/agent-defects.md#escalation-protocol). Keep these out of removal batches. Redact exposed credentials and report them for rotation. |
| [Weak tests or grader gaming](references/tests.md) | Report confirmed gaming as an SBAR blocker. Preserve the behavioral oracle and coverage floors. Never hide a failure by weakening a check or widening a mask. |

Explain bugs or design changes separately. Reuse existing authorization and ask only about effects outside it. For new branches, wrappers or unresolved ownership, use `octocode-engineer`. Coordinate multiple actors with [workflow composition](../octocode-workflows/references/composition.md).

Run and read the relevant checks for each batch. Repair an introduced failure only when new evidence changes the diagnosis. If a safe repair cannot be established, revert that batch and report an SBAR blocker. Report [removals, deferrals and checks](output.md), including unrun checks. Use the [renderer](scripts/render-report.mjs) for requested HTML.
