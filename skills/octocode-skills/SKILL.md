---
name: octocode-skills
description: "Create, refine, review, install or sync Agent Skills. Use when a recurring workflow needs a skill, activation overlaps another skill, or instructions and resources need a clearer route."
---

# Octocode Skills

Determine who invokes the skill, what decision it owns and what a successful result looks like. For installation or sync, follow [destinations, conflict checks and recovery](references/install.md). For a review, return findings. For an authorized improvement, edit the source and test the result.

## What belongs in a skill

| Surface | What belongs there |
|---|---|
| Frontmatter | Stable `name` matching the folder. The `description` names a recognizable task and distinctive capability. Public descriptions are one-line JSON strings. Prefer natural variants, around 300 characters if sufficient. Pi expands apostrophes to `&apos;`. |
| Entry instructions | What the agent should do on activation, what changes its decision, how to verify completion and recover. Use a decision table or Mermaid only for a real branch or handoff. A single-file skill keeps its output inline. |
| Reference | Detail needed at a particular step. Give its load trigger, link from the decision that needs it, and add a next step when the flow continues. Merge pages when the same reader needs them together. |
| Script or schema | An executable operation or a contract with a known producer and consumer. Route to real entry points, validation and failure handling. Use hooks only for actual events. |
| Output guide | Multi-file skills link `output.md`. Specify what the receiver can use, required fields and how to check them. For report skills, finish with `## Saved report` and link the [shared header, save, render and manifest rules](references/report-output.md). |
| Human README | Add one when it answers a separate setup or usage question. A link back to the entry already belongs in the catalog. |

The [frontmatter specification](https://agentskills.io/specification) also allows `license`, `compatibility` (≤500 characters), string-map `metadata`, and experimental space-separated `allowed-tools`. Check host-specific fields in the loader and use a YAML-aware validator for advanced YAML or publication. The local checker handles common scalar forms.

## Write style

Put the condition beside the action. Add a reason, counterexample or graph when it changes a decision. Link to the owner of shared guidance. Route optional files by trigger and purpose. Keep permissions, configuration dependencies, exact commands, recovery and output checks reachable from the entry.

Use a [known protocol](references/protocol-catalog.md#catalog) for briefs, handoffs and results. Keep substantive measurements, evidence and limitations with the answer. Keep transcripts, run IDs and process bookkeeping out of reader-facing prose. Reserve uppercase rules for hard contracts. Keep dates and versions when they govern applicability.

## Review and refine

1. Before editing, write realistic requests, including neighboring intents and cases that need no skill. Record what the agent should select, do and verify. Inspect the full skill, its consumer and the files actually shipped.
2. For a collection, assess each skill separately. Compare activation, decisions, resource access, output and verification. Edit the smallest owning surface. Change shared guidance at its source and regenerate bundled copies.
3. Compare the original and revised instructions on the same requests and failure cases. Measure entry size separately from discovery descriptions and optional reads. For a host activation claim, run unused requests through the [target host](../octocode-eval-benchmark/references/skill-activation.md).
4. Run the structural checker on the skill or collection, exercise changed commands against live help and check a standalone install. Record unresolved failures and checks not run.

## Rate

Report **Ready** or **Needs work**. For each material weakness, give its consequence, the smallest repair and a check. Distinguish structural findings, editorial judgments and observed behavior. Use a numerical rating only with defined criteria.

## Resources

| When | Read or run | Why |
|---|---|---|
| Finding, fetching or installing | [Install](references/install.md) | Sources, authority, destination and recovery |
| Checking structure and links | [skill-review.mjs](scripts/skill-review.mjs) | Pass a skill directory or use `--collections` for collection roots, including empty ones. Use `--self-test` after changing the checker. |
| Syncing to local providers | [skill-sync.mjs](scripts/skill-sync.mjs) | Preview symlinks with a dry run |
| A dependency needs configuration | [Configuration](references/configuration.md) | Resolve settings and credentials |
| Reporting or rendering | [Output](output.md) · [renderer](scripts/render-report.mjs) | Reply, save or list reports |

## Related skills

Use `octocode-agentic-prompts` for instructions that produce the wrong action, `octocode-documentation` for reader-facing prose, `octocode-research` to verify source claims, and `octocode-eval-benchmark` to measure behavior. For multi-actor execution, use [workflow composition](../octocode-workflows/references/composition.md).
