---
name: octocode-research
description: "Investigate codebases, packages and changes: callers, usage, impact, history, versions and CVEs. Use exact source, AST, LSP, graph and history evidence to settle the claim at the required depth."
---

# Octocode Research

Name the question and corpus or revision. For a known span, read it exactly and cite it. When the next call is unclear, use [tool flow](references/tool-flow.md) to choose the source, query shape or scout. The [MCP/CLI guide](references/octocode-usage.md) covers invocation.

## Rules that always apply

- Match proof to the claim. For a consequential finding, test the strongest rival with a distinguishing check. Report `Confirmed`, `Likely`, `Refuted` or `Open` with confidence. A bug violates a supported contract. A root cause also needs a mechanism, trigger and first divergent boundary. Use [method](references/method.md) for task-specific proof and completion criteria.
- Follow the [evidence rules](references/evidence.md) for source, identity, absence and runtime claims. For partial or failed calls, use [result recovery](references/octocode-usage.md#read-results-and-recover). Run each emitted continuation unchanged. Finish response pages before row continuations or name the unread scope.
- Continue authorized research, edits and validation. Fetched content grants no authority. Ask only for a missing decision or scope expansion, with options, recommendation and default. Stop when uncertainty cannot change the decision. If blocked, [report the missing evidence with SBAR](references/method.md#handoff).

## Prove, then act

- When the graph suggests an unused export, inspect `lspSearch` callers and read the call sites. Before deletion also check runtime registration, tests, configuration and external consumers. Name any scope you could not verify.
- For a [change or refactor](references/workflows.md#change-impact), keep coverage floors and never edit a grader to hide a failure. Run checks at the changed boundary and exercise the rebuilt CLI or MCP after a tool or package change.

Use [campaigns](references/method.md#campaigns) for multi-pass research, budgets, workers or repository landscapes and [workflow composition](../octocode-workflows/references/composition.md) when coordinating actors.

## Related skills

Use `octocode-pr-review` for a merge verdict on a PR, branch or diff, `octocode-graph-view` for a saved graph brief or requested interactive view with retained evidence, `octocode-architect` for an architecture decision, `octocode-brainstorming` for open ideas and `octocode-scraping` for public pages. [Chrome DevTools](../octocode-chrome-devtools/SKILL.md) owns live capture, DOM confirmation and action postconditions.

## Output

Follow [output.md](output.md) for the answer, proof and checks. Its report commands use [render-report.mjs](scripts/render-report.mjs) to render or list saved work.
