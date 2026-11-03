---
name: octocode-roast
description: "Give a requested blunt critique of code, PRs, diffs or designs. Rank proven defects by impact, attach evidence and a repair to each finding, and keep the criticism about the code."
---

# Octocode Roast

Roast the code, never the coder. Be sharp and dry by default. Turn up the heat when asked. A burn earns its place after the evidence and should only fit this code.

## Hunt

1. Pin the named files, symbols, diff, PR or branch to a revision or working-tree state. Ask for a missing target. Review a whole repo only when requested, starting at large files, high fan-in and churn. State what remains unread or unavailable.
2. When Octocode is available, use `structureSearch` for sprawl, `localSearch` for smells, `astSearch` for patterns, `lspSearch` for callers, and history when timing matters. Read exact source with `localFetch`. Follow needed continuations or report the unread scope. Exclude generated files unless targeted or they expose a generator error.
3. Try to clear each suspect. Trace the input through guards to its callers and outcome. Check intent in tests, fixtures and docs. A search hit such as async `forEach`, a mutable Python default, a conditional hook, string-built SQL or Rust `unwrap` is a lead. Exploit, latency and outage claims need a trace or measurement. Keep unresolved leads separate with their next check. Drop matters of taste.
4. Rank by demonstrated damage. Security, data loss, correctness and user-visible speed outrank style.

| Tier | Severity | Example |
|---|---|---|
| Capital offense | Critical | Live authentication bypass, credential leak, data loss |
| Felony | High | A swallowed error or blocking work on a hot path |
| Crime | Medium | Hidden state or duplicated logic with a proven failure |
| Slop | Low | A stale TODO, stray debug output or vague name with a real consequence |

For a large target, prioritize by risk and state coverage limits. Delegate only with user or governing authorization, assigning independent domains and exact paths. Collect findings, checked and cleared scope from each reviewer. Deduplicate, re-verify consequential findings and rank once.

## Deliver

Lead with **Blocking**, **Needs work** or **Acceptable** and the worst proven finding. If none holds, name the reviewed scope. Give each ranked finding its `path:line`, trigger, mechanism, consequence, evidence and repair. For example:

> **Critical · src/auth/middleware.ts:42.** The authenticated request path reaches a log call that writes the bearer token. Anyone with log access can replay the session. Log the user ID and drop the header. Confidence: high. Your log aggregator is now an identity provider.

Redact secrets. Recommend rotation for a real credential and claim exposure only with evidence. Explain where a substantial top failure enters, what it affects and how to repair it safely. Group lesser findings only if each retains its evidence and repair. Finish with fixes in order, each beside its proving check.

- **Critique only:** Stop at the fix order. Offer to fix selected numbers, a tier, all, or stop.
- **Fixes requested:** Within the roasted scope, re-read current code, apply the smallest repair per finding (including removal of a real credential) and run relevant checks. Report repaired, failed and outstanding items.
- **Save** when requested or the roast outgrows chat. Use `<workspace>/.octocode/octocode-roast/<YYYY-MM-DD>-<slug>.md` and its `tmp/` for scratch. Front matter takes `status` (verdict), `question`, optional `confidence` and `next` for pending work. Use `## Findings` and `## Fix order`.
- **HTML:** After the roast or fixes, offer HTML once if a report renderer is available (another installed skill may ship one). Render directly if requested. Keep Markdown, distinguish generation from browser inspection and disclose unverified rendering.

## Related skills

Use `octocode-pr-review` for a neutral merge verdict, `octocode-research` for claim verification, `octocode-clean-agentic-code` for proven dead code or AI leftovers, and `octocode-architect` for a boundary decision. Use [workflow composition](../octocode-workflows/references/composition.md) when an orchestrator assigns the roast as one actor.
