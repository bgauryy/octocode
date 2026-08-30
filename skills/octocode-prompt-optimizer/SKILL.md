---
name: octocode-prompt-optimizer
description: "Use when an agent prompt, tool schema, policy, or handoff needs to get clearer, safer, easier to trigger, cheaper in context, or measurable against real behavior. For SKILL.md folder install/review/structure, use octocode-skills."
---

# Octocode Prompt Optimizer

Optimize instruction behavior, not prose aesthetics.

Flow: `READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT`.

Workspace output contract: chat-only deltas stay in chat. New saved reviews or unnamed optimized drafts default to `<workspace>/.octocode/octocode-prompt-optimizer/`; scratch data uses `<workspace>/.octocode/tmp/octocode-prompt-optimizer/`. User-approved prompt, schema, policy, and source edits keep their named paths. Never fall back to a user-level Octocode home for artifacts.

## Lobby rules and gates
- READ: inspect the whole input and its type; UNDERSTAND: map goal, parts, flow, assumptions, and unknowns.
- RATE: record evidenced issues, severity, and baseline; FIX: address Critical/High issues and name deliberate deferrals.
- VALIDATE: prove intent and required behavior remain correct; OUTPUT: provide the requested artifact and truthful delta.
- Use the full path for multi-section, ambiguous, tool-facing, or high-risk work; combine adjacent steps only for short, low-risk text. Never skip VALIDATE.
- Preserve intent, working branches, identifiers, commands, and required metadata; ask before changing them.
- Verify cited commands, flags, paths, tool names, and schemas before rewriting; flag unverified claims.
- Make only critical behavior mandatory; retain preference language for real preferences. Mutate files only with authority.
- Stop when: a material unknown would change intent, scope, or risk (ask one focused question and pause); instruction authority is ambiguous, or resolving a conflict would override user intent; an edit changed intent or working logic (revert it and return to UNDERSTAND); write authority is missing (deliver a patch-style delta instead of a file change); a VALIDATE check fails twice on the same section (report the weakest branch instead of forcing a pass); a reliability gain has no held-out evidence (report it as unmeasured).

## Smart routes — load only what the current step needs
- READ and UNDERSTAND: load `references/gates.md` — read every section and map intent before judging or drafting.
- RATE: load `references/rate.md`; FIX: load `references/fix.md`; VALIDATE: load `references/validate.md`; OUTPUT: load `references/output.md` — load only the active gate so later-step advice cannot bias the current decision.
- When instructions conflict or a fix needs a compact instruction pattern, load `references/patterns.md` — apply the higher authority and log the resolution in one line.
- When reducing noise, load `references/conciseness-toolkit.md`; when fixing priority/hierarchy load `references/attention.md`; when choosing a technique for an observed failure load `references/prompt-techniques.md` — match technique to failure mechanism.
- When optimizing tool or MCP contracts, load `references/tool-contracts.md`; for agent handoffs load `references/agent-communication.md`; for typed packet boundaries load `references/zod-agent-contracts.md` — make inputs, outputs, authority, and failure states explicit.
- When context can overflow, load `references/context-budget.md`; when repeated calls share stable prefixes load `references/prompt-caching.md` — control relevance, pagination, latency, and cost.
- When reliability must be measured, load `references/evaluation-data.md` — build realistic held-out scenarios, verifiers, metrics, and a failure ledger.
- When instructions consume retrieved or user-supplied content, load `references/untrusted-content.md` — preserve the boundary between data and authority.
- When improving this skill, prefer `octocode-eval-benchmark`; otherwise load `references/improve-loop.md` — require measurable acceptance instead of intuition.

## Related routes
- Use `octocode-skills` for skill-folder architecture/review; `octocode-research` to verify cited contracts; `octocode-eval-benchmark` for held-out behavior.
- Use `octocode-subagent` for delegation topology.

## Done gate
- This skill ships no scripts: every gate above is model-driven, so never report a check you did not actually perform.
- Done requires VALIDATE passed, the OUTPUT variant matching the request, and the reported before/after score, changed files, and deferrals all matching reality.
