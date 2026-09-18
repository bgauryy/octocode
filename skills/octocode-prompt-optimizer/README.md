# Octocode prompt optimizer

Write and repair instruction surfaces so they change behavior. A stated preference leaves every choice open; a defined boundary against the behavior it is confused with decides the next action. Preserve intent when you change tool contracts or schema contracts.

## Use when

- A goal must become a compact prompt, rule, tool description, or policy.
- An instruction surface is unclear, unsafe, too expensive in context, or difficult to trigger.
- Pi, LangChain, LangGraph, a skill host, or another runtime may assemble different effective context than the edited source suggests.
- MCP server instructions, a tool description, and a schema disagree, or a shared field name drifted between tools.
- A handoff omits authority, evidence, acceptance, or return shape.
- Equivalent capabilities or payloads drift across agent apps, hosts, vendors, or protocol adapters.
- A tool schema or pagination contract permits ambiguous or incomplete behavior.
- Token, output, cache-write, tool-call, or retry costs need an explicit cost-per-success comparison.
- A context window needs a usable budget that reserves output/reasoning space and counts tools, history, and cached tokens correctly.
- OpenAI or Anthropic prompt caching misses, or a frozen agent base prompt may be drifting between workers.
- Accumulated context must be compacted, summarized, or compressed without destroying evidence.
- Reliability needs behavioral evaluation rather than wording judgment alone.

## Example

Weak: “Be efficient with tools.”

Decidable: “Reuse a schema already read; inspect it again only when the tool or schema version changes.”

The skill first resolves the executing surface, runtime dependency, and effective context flow. It then separates prompt wording, context budgeting, tool/MCP contracts, agent contracts, and evaluation into load-on-demand reference domains.

## Workflow

```text
READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT
```

Normal work follows the full flow. Active safety, permission, or production failures use the lobby's containment branch, then return for broader rating and cleanup. Claims of improved reliability need a fixed evaluation and measured comparison.

## Install

```bash
npx -y octocode skill install octocode-prompt-optimizer
```

## Maintainer verification

Run the `octocode-skills` review against this folder.
