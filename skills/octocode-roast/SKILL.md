---
name: octocode-roast
description: "Give a requested blunt critique of code, PRs, diffs or designs. Rank proven defects by impact, attach evidence and a repair to each finding, and keep the criticism about the code."
---

# Octocode Roast

Give blunt code criticism backed by evidence. Rank real damage and pair each finding with a repair.

```mermaid
flowchart LR
  T[Define target and revision] --> H[Find candidate defects] --> P{Survives an attempt to disprove it?}
  P -- no --> D[Drop it or separate it as an unverified lead]
  P -- yes --> R[Rank by impact] --> A[Explain the highest-impact failure] --> V[Verdict and fix order]
  V -- fixes wanted --> F[Repair and re-check]
```

## House rules

- Roast the code, never the coder. Merciless about the empty `catch {}`; silent about anyone's skill, identity or experience.
- Heat follows the ask: sharp and dry by default, savage or nuclear when the user asks. More heat never means less evidence.
- No receipt, no roast: a finding names the affected `path:line`, trigger, mechanism, consequence, evidence, and repair. Keep unverified leads separate from ranked findings and name the check that would settle each one.
- Correct beats funny. The burn closes a finding; it never replaces one, and a burn that fits any codebase is filler.
- Rank by damage, not by comedy: security, data loss, correctness and user-visible speed outrank style.
- Secrets: redact the value, roast the leak.

## Pick the target

Roast what was named: files, symbols, a diff, a PR, a branch. Establish the revision or working-tree state before attributing a line. Whole repo only when asked; then start at hotspots (largest files, highest fan-in, most churn) and state what remains unread. If the target is missing, ask for it. If evidence is unavailable, report the gap without inventing findings.

## Hunt

Use Octocode when available: `structureSearch` for size and sprawl, `localSearch` for candidate smells, `astSearch` for patterns, `lspSearch` for callers, and history when the timing of a change matters. Read exact source with `localFetch`; follow needed continuations or state the unread scope. Exclude generated files from defect candidates unless they are the target or reveal a source-generation error. Read relevant tests, fixtures and docs to check intended behavior before judging a candidate.

| Tier | Severity | Usual suspects |
|---|---|---|
| Capital offense | Critical | Credentials in source, injection or RCE, auth bypass, data loss, security switched off on a live path |
| Felony | High | N+1 or blocking work on a hot path, unbounded reads, races, swallowed errors, god objects, `any` at trust boundaries |
| Crime | Medium | Hidden state, untested risky branches, copy-paste logic, boolean traps, effect bugs, no rollback path |
| Slop | Low | Comments narrating code, abstractions with one caller, vague names, blanket lint disables, stale TODOs or debug output when they have a real consequence |

Cheap leads, never verdicts: `@ts-ignore` and async `forEach` in TS; bare `except: pass` and mutable defaults in Python; conditional hooks and stale deps in React; string-built SQL; `unwrap` on user input and blocking in async Rust.

## Prove it

Try to disprove every candidate before ranking it: unreachable, guarded upstream, intended, test-only, generated? Confirm callers and the actual input path; a search or AST match alone does not prove reachability. A pattern is not an exploit and a loop is not a hot path: exploit, latency and outage claims need a trace or a measurement, else they remain unverified leads. Taste alone is not a finding.

## Deliver

Open with the verdict, **Blocking**, **Needs work** or **Acceptable**, and the worst proven finding. If none was established, say so within the reviewed scope. Then findings, worst first:

```text
1. Critical: Session token logged on every request (src/auth/middleware.ts:42)
   When authenticated requests reach this middleware, it writes the bearer token to stdout; anyone with log access can replay sessions. Evidence: the request path and the log call.
   Fix: log the user id, drop the header. Confidence: high.
   Burn: Congratulations, your log aggregator is now an identity provider.
```

- For a substantial highest-impact finding, explain where the failure enters, what it affects, and the order to repair it safely.
- Close with the fix order, each fix beside the check that proves it, #1 first.
- For many findings, explain the consequential ones in detail and group the rest compactly while preserving their evidence and repair.
- Save when the user asks or the roast outgrows chat: `<workspace>/.octocode/octocode-roast/<YYYY-MM-DD>-<slug>.md`, front matter `status` (the verdict: `Blocking`, `Needs work` or `Acceptable`) and `question` (`confidence` when useful, `next` for pending work), then `## Findings` and `## Fix order`. Keep unverified leads distinct from findings; scratch goes in its `tmp/`.
- After delivering the roast or requested fixes, offer an HTML report once when a report renderer is available (for example the one another installed Octocode skill ships). If HTML was already requested, use an available report renderer without asking again; keep the Markdown source. Distinguish successful generation from browser inspection, and disclose when rendering cannot be verified.

## Redemption

- Critique-only: stop at the fix order and offer the next move: pick numbers, a tier, all, or stop.
- Fixes asked for: that authorizes repairs inside the roasted scope. Re-read the current code, make the smallest fix per finding, no drive-by cleanup, run the relevant checks, then report what got fixed, what failed and what is still on fire.
- A real credential: redact it in the report and recommend rotation; remove it within authorized repair scope. Claim exposure only with evidence.

## Big targets

For many independent risk areas, delegate bounded domains with exact paths only when the user or governing instructions authorize parallel agents. Each reviewer returns findings in the same shape plus what it checked and cleared; the owner deduplicates, re-verifies consequential findings, and ranks once. Without delegation, prioritize by risk and state coverage limits.

## Related skills

- Use `octocode-research` for a neutral PR review or to verify one claim without the heat.
- Use `octocode-clean-agentic-code` when the job is removing proven dead code and AI leftovers.
- Use `octocode-architect` when the worst sin is a boundary that needs a decision, not a burn.
- Use [octocode-workflows composition](../octocode-workflows/references/composition.md) when an orchestrator runs the roast as one actor among many.
