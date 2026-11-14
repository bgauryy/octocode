---
name: octocode-clean-agentic-code
description: "Use when removing dead code, unused exports, duplicate helpers, debug logs, agent leftovers, chat residue or AI-writing tells without changing behavior, including instruction, configuration and weakened-test cleanup. Every removal needs proof the code or text is unused."
---

# Octocode clean agentic code

1. **Establish the baseline.** Confirm the authorized scope, inspect existing edits and run the affected checks before changing code when possible. Record existing failures or a blocked baseline; do not assume a clean starting state. Preserve work that is not yours.
2. **Prove the candidate.** Identify what removal could break. Check semantic references, public entrypoints, configuration, dynamic loading and external consumers with [Octocode discovery and exact reads](references/octocode-usage.md). Graph edges, file size, pattern matches and empty searches alone cannot prove redundancy; name unavailable evidence and unscanned scope.
3. **Decide: keep, defer or remove.** Keep live contracts. Defer unresolved candidates with the check that would settle them. Remove only proven redundancy. Put the deciding evidence and material uncertainty beside the decision; no separate artifact is required.
4. **Apply an owned batch.** Keep changes reversible and reviewable: a cleanup that adds more lines than it removes, or rewrites beyond its candidates, needs a stated reason. Mark removals on high-stakes paths (authentication, authorization, data deletion, billing, public API) for human review. Keep one owner, update callers and links, and regenerate derived files at their source. Cleanup authorization does not authorize behavior changes or unrelated refactors.
5. **Verify against the baseline.** Run and read the affected checks and inspect the final diff for unintended changes. Distinguish existing failures from introduced ones. If a failure invalidates the removal proof, revise the diagnosis; if no safe in-scope repair is established, undo only your batch's edits, preserving pre-existing work, and report an SBAR blocker.

| Candidate | Decision |
|---|---|
| [Dead or duplicate code](references/code-smells.md) | Check the candidate's removal bar and preserve supported consumer behavior. |
| [Comments, instructions or configuration](references/text-config-hygiene.md) | Keep text and configuration with a parser, runtime or external consumer until its contract deliberately changes. For AI-writing tells, chat residue and placeholders, use [writing tells](references/writing-tells.md). If cleaner wording changes agent behavior, use `octocode-agentic`. For skill packaging or activation, use `octocode-skills`. |
| [Agent leftovers or defects](references/agent-defects.md) | Report error masking and reachable stubs as [SBAR blockers](references/agent-defects.md#escalation-protocol). Keep these out of removal batches. Redact exposed credentials and report them for rotation. |
| [Weak tests or grader gaming](references/tests.md) | Report confirmed gaming as an SBAR blocker. Preserve the behavioral oracle and coverage floors. Never hide a failure by weakening a check or widening a mask. |

Explain bugs or design changes separately. Reuse existing authorization and ask only about effects outside it. For new branches, wrappers or unresolved ownership, use `octocode-engineer`. Coordinate multiple actors with `octocode-workflows`.

Done when the final diff contains only authorized changes and supported behavior and guardrails survive the affected checks. Report [removals, deferrals and checks](output.md), including baseline failures, unrun checks and remaining uncertainty; a blocked check is not a pass, and fewer lines alone prove no gain. Run the [renderer](scripts/render-report.mjs) for requested HTML.
