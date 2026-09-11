---
name: octocode-prompt-optimizer
description: "Use when a prompt, agent or MCP instruction, tool/schema description, policy, or handoff must actually change behavior: decidable boundaries, cut no-op text, put rules in their owning layer, audit tools for contradictions and drift, budget context. SKILL.md structure, install, or review: use octocode-skills."
---

# Octocode prompt optimizer

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-eval-benchmark`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Optimize instruction behavior, not prose aesthetics. A rule that states a preference changes nothing — "be efficient with tools" leaves every call open; "reuse a schema you already fetched; fetch only for an unfamiliar tool" decides the next call.

Flow: `READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT`.

Make each rule decide an observable action. Use the questions in `references/behavior.md` to resolve ambiguity; a rule does not need five labeled parts. When the input is a goal rather than an existing prompt, skip RATE.

Reviews/drafts: `<output>/octocode-prompt-optimizer/`; scratch: `<output>/tmp/octocode-prompt-optimizer/`. Chat-only deltas stay in chat; approved prompt/schema/policy/source edits keep their paths.

## Rules
- Read the complete input and map its intent before judging it. Rate evidenced issues before drafting fixes.
- For short, low-risk text, combine adjacent phases. For complex, tool-facing, or risky instructions, keep the phases explicit. Always validate the finished draft.
- Keep a sentence only when it defines a distinction, sets a boundary, explains a consequence, or directs an action. Cut repeated rules, motivational language, role-play, uninformative headings, and decorative terminology.
- Prefer the smallest wrong/right example pair when it defines the boundary better than more prose. Use literal language when it suffices.
- State intent before constraints or steps. Use grammatical sentences, concrete nouns, explicit referents, and direct verbs that name the action.
- Use standard short terms (`repo`, `config`, `env`) when meaning stays exact. Keep one noun per concept; replace noun phrases with direct verbs (`decide`, `verify`).
- Use an available small, fast model (for example, Luna or Haiku) for bounded repetition, term-drift, format, and checklist scans. Give it exact input and a fixed output shape; verify each finding before editing. Keep intent mapping, conflict resolution, risky rules, and final validation on the main model.
- Maximize behavior per token, not brevity. Justify growth by the boundary it adds.
- Preserve intent, working branches, identifiers, commands, and required metadata. Verify technical claims before rewriting them.
- Reserve mandatory language for real requirements. Keep preferences flexible and mutate files only when authorized.
- When the request is for prompt text, output only that text.
- Ask one focused question only when an unresolved choice changes intent, scope, or risk. Without write authority, return a delta. Report unmeasured reliability claims as unmeasured.

## Smart routes — load only what the current step needs

Load references that resolve the current decision. Reuse material already read and combine independent reads when useful.

| When | Load | It decides |
|---|---|---|
| READ, UNDERSTAND | `references/gates.md` | intent map before any judgment or draft |
| RATE · FIX · VALIDATE · OUTPUT | `references/rate.md` · `references/fix.md` · `references/validate.md` · `references/output.md` | severity, repair, gate checks, delivery variant |
| A rule leaves the next action ambiguous | `references/behavior.md` | observable action, scope, and useful examples |
| Instructions conflict, or a fix needs a stock pattern | `references/patterns.md` | which authority wins; one-line resolution log |
| Text is noisy, buried, or mis-prioritized | `references/conciseness-toolkit.md` · `references/attention.md` | token cuts that keep logic; rule placement |
| A specific failure mode is observed | `references/prompt-techniques.md` | technique matched to failure mechanism |
| MCP server instructions, tool description, or schema | `references/tool-contracts.md` | layer ownership: workflow vs. when-to-call vs. fields |
| Multi-tool server, or after any description/schema edit | `references/contract-audit.md` | set-wide contradictions, overlapping selection, descriptor drift |
| Agent handoffs; typed packet boundaries | `references/agent-communication.md` · `references/zod-agent-contracts.md` | inputs, outputs, authority, failure states |
| Context can overflow; repeated calls share a prefix | `references/context-budget.md` · `references/prompt-caching.md` | relevance, pagination, latency, cost |
| Accumulated context must be compacted, summarized, or compressed | `references/compaction.md` | what to cut, when to compact, what stays retrievable |
| A token saving, compression ratio, or context-cost claim needs proof | `references/token-measurement.md` | tokens per fact, task-specific comparison, and verification |
| A reliability claim needs proof | `references/evaluation-data.md` | held-out scenarios, verifiers, metrics, failure ledger |
| Instructions consume retrieved or user-supplied content | `references/untrusted-content.md` | the boundary between data and authority |
| Improving this skill | `octocode-eval-benchmark`; if unavailable, freeze goal/KPI/baseline and use comparable accept/revert evidence | measurable acceptance instead of intuition |

## Related routes
- Use `octocode-skills` for skill-folder architecture/review and `octocode-eval-benchmark` for held-out behavior. To verify technical contracts, `octocode-research` owns the MCP/CLI workflow and live tool/grammar discovery.
- Use `octocode-subagent` for delegation topology.

## Done
This skill ships no scripts. Report only checks performed; the deliverable, score, changed files, and deferrals must match reality.
