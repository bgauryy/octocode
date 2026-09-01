---
name: octocode-prompt-optimizer
description: "Use when an agent prompt, tool schema, policy, or handoff needs to get clearer, safer, easier to trigger, cheaper in context, or measurable against real behavior. For SKILL.md folder install/review/structure, use octocode-skills."
---

# Octocode Prompt Optimizer

Optimize instruction behavior, not prose aesthetics.

Flow: `READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT`.

Workspace output contract: chat-only deltas stay in chat. New saved reviews or unnamed optimized drafts default to `<workspace>/.octocode/octocode-prompt-optimizer/`; scratch data uses `<workspace>/.octocode/tmp/octocode-prompt-optimizer/`. User-approved prompt, schema, policy, and source edits keep their named paths. Never fall back to a user-level Octocode home for artifacts.

## Operating rules
- Read the complete input and map its intent before judging it. Rate evidenced issues before drafting fixes.
- For short, low-risk text, combine adjacent phases. For complex, tool-facing, or risky instructions, keep the phases explicit. Always validate the finished draft.
- Preserve intent, working branches, identifiers, commands, and required metadata. Verify technical claims before rewriting them.
- Reserve mandatory language for real requirements. Keep preferences flexible and mutate files only when authorized.
- Ask one focused question only when an unresolved choice would materially change intent, scope, or risk. Without write authority, return a delta. Report unmeasured reliability claims as unmeasured.

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

## Done
This skill ships no scripts. Report only checks actually performed; the deliverable, score, changed files, and deferrals must match reality.
