# Octocode prompt optimizer

Write and repair instruction surfaces (prompts, rules, tool descriptions, schemas, policies, handoffs) so they change behavior. A stated preference leaves every choice open; a boundary against the behavior it is confused with decides the next action.

Example. Weak: “Be efficient with tools.” Decidable: “Reuse a schema already read; inspect it again only when the tool or schema version changes.”

Workflow: `READ → UNDERSTAND → RATE → FIX → VALIDATE → OUTPUT`. Active failures are contained first. Reliability claims need `octocode-eval-benchmark`. Detail loads on demand from `references/` (flow, writing, tools, agents, context).

```bash
npx -y octocode skill install octocode-prompt-optimizer
```

Maintainers: run the `octocode-skills` review against this folder.
