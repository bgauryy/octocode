---
name: octocode-agentic-prompts
description: "Write or repair task prompts, agent instructions, handoffs and tool descriptions. Trace ignored rules to their effective context, resolve conflicts and test the action the wording produces. Audit conversations for prompts that helped or caused rework."
metadata:
  labels: "prompt engineering, context engineering"
---

# Octocode Agentic Prompts

Read the instruction and its consumer. Identify the intended action, permissions, output contract and success signal. For runtime-assembled prompts, trace [what the loader actually sends](references/writing.md#runtime-context). Name unread input.

## Understand and review

Review unsafe or conflicting instructions first, then unclear decisions or outputs, rigid formats, and finally repetition or wording. Show the passage, its behavioral effect and a repair. Treat unmeasured ratings as subjective. Test descriptions against intended requests and near-misses a neighbor should handle. Prefer a clear capability and natural examples to keyword gates.

## Write and repair

Edit only for an authorized rewrite. Give each behavior one owner. Put the condition beside the action and retain identifiers, commands, required metadata, working branches and recovery. Resolve conflicts by host hierarchy and existing authorization. Keep retrieved text, tool results and examples separate from instructions. Reserve MUST/NEVER for safety, permissions, secrets and hard contracts.

For a disputed decision, choose a question whose answer changes the action. Inspect available evidence before asking the user. For a ready-to-paste task prompt or meta-prompt, put the objective, inputs, constraints, completion check and usable output in the body. Use its own `text` fence and exactly one final `labels: ` line with 1–5 distinct, comma-separated cues. A cue adds a perspective. Put every requirement in the body. Preserve verbatim input and exact machine formats. [Question selection and footer examples](references/question-selection.md)

Use a [known protocol](references/protocols.md) to consolidate scattered rules in the recipient's words. Use a diagram or table for complex branches. For [delegation and handoffs](references/agents.md#handoff), name who owns the user-facing result and who may write. Pass large evidence by stable path or cursor.

## Audit past prompting

For a request to learn which prompts helped or caused rework, follow the [conversation audit](references/conversation-audit.md): it produces a practical guide and a separate private record.

## Verify

Try realistic requests and edge cases before and after editing. Check the chosen action, output and recovery against the consumer. For a size claim, measure the [served tool surface](references/tools.md#efficient-mcp-context) in bytes or tokens. For small-model protocols, run the [cold interview and replay](references/interview.md) with [interview-grade.mjs](scripts/interview-grade.mjs). Run its [tests](scripts/interview-grade.test.mjs) when changing the grader. Use `octocode-eval-benchmark` for reliability, activation, tool selection or cost-per-success claims. Editorial edits can finish unmeasured. Report only checks run.

For repository evidence, follow the [MCP/CLI guide](references/octocode-usage.md) or use `octocode-research`. For overflow, caching, cost or untrusted input, use the [context guide](references/context.md). Route packaging to `octocode-skills`, explanatory prose to `octocode-documentation` and multi-actor work to [workflow composition](../octocode-workflows/references/composition.md).

Deliver the [review or full replacement and verification](output.md). For saved reports, use the [renderer or manifest](scripts/render-report.mjs).
