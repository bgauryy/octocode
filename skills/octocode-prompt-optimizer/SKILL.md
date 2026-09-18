---
name: octocode-prompt-optimizer
description: "Use when a prompt, agent contract, MCP instruction, tool/schema description, policy, or handoff must change behavior: resolve context flow, make boundaries decidable, align runtimes, remove no-op text, place rules correctly, audit drift, and budget context. For SKILL.md structure or trigger review, use octocode-skills."
---

# Octocode prompt optimizer

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-eval-benchmark`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load/run a reference, doc, or script only when it changes the next action; otherwise keep the rule here.

Optimize the instruction surface the runtime reads, not nearby prose. Trace `source → assembly/serialization → model or tool reader → observable action/result`, then change the smallest owning layer.

Flow: `READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT`

When the input is a goal rather than an existing prompt, skip RATE.

Reviews/drafts: `<output>/octocode-prompt-optimizer/`; scratch: `<output>/tmp/octocode-prompt-optimizer/`. Chat-only deltas stay in chat; approved prompt/schema/policy/source edits keep their paths.

## Operating context and urgency

Before judging text, record the context that changes the optimization:

| Field | Record |
|---|---|
| Target | exact prompt, instruction, tool/schema, policy, or handoff and its owning source |
| Runtime | executing surface, host/framework version, assembly, caching, and reader |
| Readers | model, agent, tool client, server, human, or downstream parser and their authority boundaries |
| Outcome | observable behavior to change and evidence of the current failure |
| Invariants | intent, frozen contracts, identifiers, permissions, and working branches |
| Delivery | output, write authority, budget, and success checks |
| Urgency | active safety, permission, or production failure versus normal improvement |

For an active safety, permission, or production failure, contain first: `READ affected source → UNDERSTAND authority/invariants → FIX the smallest reversible critical rule → VALIDATE the affected branch → OUTPUT`. Then return to RATE for broader work. Urgency never expands authority, permits a partial read of the affected source, or skips validation.

## Rules
- Read the complete input and map its intent before judging it. Rate evidenced issues before drafting fixes.
- Identify the executing surface and prove the running dependency and effective context path before counting or rewriting context. A manifest, source file, or installed copy alone does not prove what the active process reads.
- For short, low-risk text, combine adjacent phases. For complex, tool-facing, or risky instructions, keep the phases explicit. Always validate the finished draft.
- Make every rule decide an observable action. Keep one owner per behavior; use `references/writing/behavior.md` only when its action or scope remains ambiguous.
- Maximize behavior per token, not brevity. Justify growth by the boundary it adds.
- Treat context capacity, token billing, cache reuse, and prompt integrity as separate constraints.
- Preserve intent, working branches, identifiers, commands, and required metadata. Verify technical claims before rewriting them.
- Reserve mandatory language for real requirements. Keep preferences flexible and mutate files only when authorized.
- When the request is for prompt text, output only that text.
- Ask one focused question only when an unresolved choice changes intent, scope, or risk. Without write authority, return a delta. Report unmeasured reliability claims as unmeasured.

## Smart routes — load only what the current step needs

Load references that resolve the current decision. Reuse material already read and combine independent reads when useful.

| When | Load | It decides |
|---|---|---|
| READ, UNDERSTAND | `references/flow/gates.md` | intent and runtime-context map before judgment |
| RATE | `references/flow/rate.md` | evidenced severity and baseline score |
| FIX | `references/flow/fix.md` | smallest repair in the owning layer |
| VALIDATE | `references/flow/validate.md` | applicable behavioral and domain gates |
| OUTPUT | `references/flow/output.md` | delivery variant and truthful delta |
| A rule leaves the next action ambiguous | `references/writing/behavior.md` | observable action, scope, and useful examples |
| Instructions conflict, or a fix needs a stock pattern | `references/writing/patterns.md` | which authority wins; one-line resolution log |
| Text is noisy, buried, or mis-prioritized | `references/writing/conciseness-toolkit.md` · `references/writing/attention.md` | token cuts that keep logic; rule placement |
| A specific failure mode is observed | `references/writing/prompt-techniques.md` | technique matched to failure mechanism |
| A host, framework, skill loader, middleware, graph, or dependency assembles the context | `references/flow/runtime-context.md` | executing surface, running version, visibility, lifetime, and effective model/tool input |
| MCP server instructions, tool descriptions, or schema design | `references/tools/tool-contracts.md` | ownership: workflow vs. selection vs. exact fields |
| MCP discovery, negotiated versions, calls, results, caching, or state | `references/tools/mcp-wire-contract.md` | version-specific wire and lifecycle contract |
| Multi-tool server, or after any description/schema edit | `references/tools/contract-audit.md` | set-wide contradictions, overlapping selection, descriptor drift |
| Agent delegation, handoff, async work, or capability exchange | `references/agents/agent-communication.md` | ownership, lifecycle, authority, recovery, context transfer |
| The same capability or payload crosses agent apps, hosts, vendors, or protocols | `references/agents/cross-app-contracts.md` | canonical semantics, native adapters, compatibility, and removal gates |
| A TypeScript/Zod agent or MCP packet needs a runtime schema | `references/agents/zod-agent-contracts.md` | discriminated states, bounds, validation, versioning |
| Context can overflow or the usable working budget is unclear | `references/context/context-budget.md` | capacity, occupancy, output reserve, relevance, pagination |
| Token use, model choice, caching, or tool use needs an economic decision | `references/context/token-economics.md` | cost per successful task at the measured operating point |
| Repeated OpenAI or Anthropic calls share a prefix, or cache hits are missing | `references/context/prompt-caching.md` | vendor controls, invalidators, telemetry, and break-even inputs |
| An agent base prompt must remain frozen across tasks or workers | `references/agents/agent-prompt-integrity.md` | versioned base, append-only overlays, digest verification |
| Accumulated context must be compacted, summarized, or compressed | `references/context/compaction.md` | what to cut, when to compact, what stays retrievable |
| A token saving, compression ratio, or context-cost claim needs proof | `references/context/token-measurement.md` | tokens per fact, task-specific comparison, and verification |
| A reliability claim needs proof | `references/flow/evaluation-data.md` | held-out scenarios, verifiers, metrics, failure ledger |
| Instructions consume retrieved or user-supplied content | `references/context/untrusted-content.md` | the boundary between data and authority |
| Improving this skill | `octocode-eval-benchmark` | — |

## Related routes
- Use `octocode-skills` for skill-folder architecture/review and `octocode-eval-benchmark` for held-out behavior. To verify technical contracts, `octocode-research` owns the MCP/CLI workflow and live tool/grammar discovery.
- Use `octocode-subagent` for delegation topology.

## Done
This skill ships no scripts. Report only checks performed; the deliverable, score, changed files, and deferrals must match reality.
