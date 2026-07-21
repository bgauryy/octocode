---
name: octocode-prompt-optimizer
description: "Use when an agent prompt, skill, tool schema, policy, or handoff needs to get clearer, safer, easier to trigger, cheaper in context, or measurable against real behavior."
---

# Octocode Prompt Optimizer

Optimize instruction behavior, not prose aesthetics. Flow: `READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT`.

## Lobby rules and gates
- READ: inspect the whole input and its type; UNDERSTAND: map goal, parts, flow, assumptions, and unknowns.
- RATE: record evidenced issues, severity, and baseline; FIX: address Critical/High issues and name deliberate deferrals.
- VALIDATE: prove intent and required behavior remain correct; OUTPUT: provide the requested artifact and truthful delta.
- Use the full path for multi-section, ambiguous, tool-facing, or high-risk work; combine adjacent steps only for short, low-risk text. Never skip validation.
- Preserve intent, working branches, identifiers, commands, and required metadata; ask before changing them.
- Verify cited commands, flags, paths, tool names, and schemas before rewriting; flag unverified claims.
- Make only critical behavior mandatory; retain preference language for real preferences. Mutate files only with authority.

## Smart routes — load only what the current step needs
- During the core path, load only the active gate: `references/gates.md`, `references/rate.md`, `references/fix.md`, `references/validate.md`, or `references/output.md` — prevent later-step advice from biasing the current decision.
- When reducing noise, load `references/conciseness-toolkit.md`; when fixing priority/hierarchy load `references/attention.md`; when choosing reusable structures load `references/patterns.md` or `references/prompt-techniques.md` — match technique to failure mechanism.
- When optimizing tool or MCP contracts, load `references/tool-contracts.md`; for agent handoffs load `references/agent-communication.md`; for typed packet boundaries load `references/zod-agent-contracts.md` — make inputs, outputs, authority, and failure states explicit.
- When context can overflow, load `references/context-budget.md`; when repeated calls share stable prefixes load `references/prompt-caching.md` — control relevance, pagination, latency, and cost.
- When reliability must be measured, load `references/evaluation-data.md` — build realistic held-out scenarios, verifiers, metrics, and a failure ledger.
- When instructions consume retrieved or user-supplied content, load `references/untrusted-content.md` — preserve the boundary between data and authority.
- When improving this skill, prefer `octocode-eval`; otherwise load `references/improve-loop.md` — require measurable acceptance instead of intuition.

## Related routes
- Use `octocode-skills` for skill-folder architecture/review; `octocode-research` to verify cited contracts; `octocode-eval` for held-out behavior.
- Use `octocode-subagent` for delegation topology; `octocode-awareness` for coordinated instruction-file edits.
