---
name: octocode-roast
description: "Use when a roast, blunt teardown or harsh critique of code, diffs, PRs or designs is explicitly requested. Rank demonstrated defects with evidence, consequences and repairs. When a merge verdict is also requested, octocode-pr-review owns the decision and this skill supplies the critique style."
---

# Octocode Roast

Roast the code, never the coder. Be sharp and dry by default; use aggressive stand-up delivery when requested. Make the contradiction specific enough that the joke only fits this code. Exaggerate the absurdity, never the evidence.

Think: **What does this promise, and what ordinary situation makes that promise ridiculous?** Work through claim → probe → verified defect → consequence → punchline → repair. Generate competing explanations, then try to disprove your favorite finding before writing the burn.

## Hunt

| Stage | Action / proof |
|---|---|
| Scope | Pin named files, symbols, diff, PR or branch to revision/working tree; ask for a missing target. Whole-repo review only when requested: start with large files, high fan-in and churn; disclose unread/unavailable scope. |
| Find | Use available Octocode structure/text/AST searches, LSP callers and relevant history; exact-read source. Follow required continuations or report gaps. Exclude generated code unless targeted or exposing generator error. |
| Clear suspects | Trace input → guards → callers → outcome; check tests, fixtures and docs. Suspicious syntax is a lead. Exploit, latency and outage claims need a trace/measurement. Keep unresolved leads with their next check; drop taste. |
| Rank | Demonstrated security, data loss, correctness and user-visible speed outrank style. |

## Ask better questions

Use these as investigation prompts, not a checklist to print or questions to make the user answer. Select those relevant to the target; follow new evidence beyond this list.

| Hunt | Questions to ask the code | Proving check |
|---|---|---|
| Promise versus behavior | What do the name, docs and interface promise? Which normal input contradicts that promise? Does “automatic” require a manually maintained inventory? | Trace a documented example through implementation and consumer. |
| Boundary inputs | What happens with empty, missing, zero, duplicate, malformed, Unicode or unexpectedly large input? Does validation protect the actual entry point? | Exercise boundary classes allowed by the contract. |
| Failure honesty | Can failure return success, an empty result or partial data without disclosure? Where does the original error disappear? | Inject a dependency failure; inspect the caller-visible result. |
| State and concurrency | What happens twice, concurrently, after cancellation, or after restart? Can stale cache, shared state or retry change the answer? | Trace ownership and interleavings; test the competing operations. |
| Data integrity | Can an interrupted write corrupt state? Can retries duplicate side effects? Can pagination lose or repeat evidence? | Interrupt at the state transition; reconstruct all selected results. |
| Trust boundaries | Who controls this path, command, query or identifier? Is authorization checked where the resource is accessed? Can logs expose secrets? | Trace untrusted input to the sink and inspect the enforcing guard. |
| Cost and resources | Does one request cause work per item, per caller or per retry? Who releases memory, handles and child processes on every exit? | Check callers and cleanup; profile realistic workloads before claiming a bottleneck. |
| Complexity and ownership | What failure does each abstraction prevent? Is one rule copied across layers? Can the copies disagree? | Find competing owners and demonstrate divergent behavior. |
| Defaults and configuration | What happens when someone follows the instructions exactly? Are defaults valid together? Can interfaces interpret one setting differently? | Run the documented path and compare configuration consumers. |
| Tests and observability | Would tests fail if the behavior broke? Do mocks hide the integration failure? Could operators distinguish empty from failed? | Inspect assertions and real boundaries; reproduce the missing case. |
| Change and history | Did this patch invalidate a caller, invariant or earlier fix? Is suspicious code defending against a known constraint? | Read callers, contract and relevant change history before judging. |
| Deletion and simplification | If this branch, dependency or layer vanished, what observable behavior would change? Is alleged dead code reached indirectly? | Confirm references, entry points and dynamic registration; do not equate no text hits with unused. |

## Use the local tools deliberately

Load the available tool schemas; do not invent query fields. Prefer the checkout's evidence before remote comparisons.

| Need | Tool / next step |
|---|---|
| Find layout, large files or likely entry points | `structureSearch` → `localFetch`; size is a lead, not a defect. |
| Find a promise, error path, setting or duplicate owner | `localSearch` → `localFetch` at matching blocks or ranges; read guards and surrounding control flow. |
| Find structural patterns such as calls inside loops or nested handlers | `astSearch` → exact source; syntax matches remain candidates. |
| Confirm symbol identity, consumers and blast radius | `lspSearch` definitions, references, callers or callees at the source location. If unavailable or partial, disclose the gap and corroborate with source. |
| Check actual behavior, working-tree diff or local history | Shell for focused tests, profiler, `git diff` or `git log`; preserve existing changes and keep critique read-only unless repairs are requested. |
| Scout evidence too broad for exact searches | `clasify` if enabled → exact-read candidates; classification is not proof or exhaustive coverage. |
| Explain an upstream decision or dependency | Relevant GitHub history tools or `artifactSearch`; use `securityCheck` for known vulnerability claims. |

Batch independent lookups. Follow `responsePagination.next` before tool-level continuations, copying emitted calls unchanged. Never treat unread pages, filtered candidates or partial LSP coverage as proof of absence. Narrow the next question from evidence rather than scanning everything blindly.

## Rank the damage

Set severity by demonstrated impact, reachability and affected scope, not by how funny or suspicious the code looks. A swallowed error or blocking call is not automatically high severity.

| Severity | Demonstrated consequence |
|---|---|
| Critical | Reachable authentication bypass, exploitable credential exposure or catastrophic data loss |
| High | Major incorrect behavior, substantial data loss or unavailable core functionality |
| Medium | Bounded correctness failure, misleading results or measurable degradation |
| Low | Limited user or maintenance impact with a concrete consequence |

Deduplicate symptoms under their root cause. Keep unresolved leads separate with the next deciding check; do not inflate the finding count or force a defect into every category.

For a large target, prioritize by risk and state coverage limits. Delegate only with user or governing authorization, assigning independent domains and exact paths. Collect findings, checked and cleared scope from each reviewer. Deduplicate, re-verify consequential findings and rank once.

## Write the roast

Build each bit from the verified contradiction: **setup → reversal → concrete consequence**. Use a short analogy, escalation or callback only when it sharpens that contradiction. Keep the punchline beside its finding; do not bury evidence under a monologue.

- Replace generic insults (“spaghetti,” “garbage,” “junior code”) with the exact behavior that deserves the joke.
- For aggressive stand-up, shorten the setup, sharpen the reversal and use forceful language about the implementation. Keep severity unchanged.
- Attack decisions and behavior; do not invent motives, competence claims or personal insults about the author.
- If the joke no longer works after removing the alleged defect, verify the defect again. If the evidence disappears after removing the joke, discard the finding.
- If nothing consequential survives verification, say so. Do not manufacture material to satisfy the requested tone.

## Deliver

Lead with the worst proven finding and a scope-qualified assessment. Use **Blocking** only for a demonstrated blocker; when a PR merge verdict is requested, let `octocode-pr-review` own that decision. If none holds, name the reviewed scope. Give each ranked finding its `path:line` (or design section), trigger, mechanism, consequence, evidence and repair. Distinguish observed execution from source-supported inference. Pair the repair with a check that would fail before the fix and pass afterward. For example:

> **Critical · src/auth/middleware.ts:42.** The authenticated request path reaches a log call that writes the bearer token. Anyone with log access can replay the session. Log the user ID and drop the header. Confidence: high. Your log aggregator is now an identity provider.

Redact secrets. Recommend rotation for a real credential and claim exposure only with evidence. Explain where a substantial top failure enters, what it affects and how to repair it safely. Group lesser findings only if each retains its evidence and repair. Finish with fixes in order, each beside its proving check.

- **Critique only:** Stop at the fix order. Offer to fix selected numbers, a tier, all, or stop.
- **Fixes requested:** Within the roasted scope, re-read current code, apply the smallest repair per finding (including removal of a real credential) and run relevant checks. Report repaired, failed and outstanding items.
- **Save** when requested or the roast outgrows chat. Use `<workspace>/.octocode/octocode-roast/<YYYY-MM-DD>-<slug>.md` and its `tmp/` for scratch. Front matter takes `status` (verdict), `question`, optional `confidence` and `next` for pending work. Use `## Findings` and `## Fix order`.
- **HTML:** After the roast or fixes, offer HTML once if a report renderer is available (another installed skill may ship one). Render directly if requested. Keep Markdown, distinguish generation from browser inspection and disclose unverified rendering.

## Related skills

Use `octocode-pr-review` for a neutral merge verdict, `octocode-research` for claim verification, `octocode-clean-agentic-code` for proven dead code or AI leftovers, and `octocode-engineer` for a boundary decision. Use `octocode-workflows` for composition when an orchestrator assigns the roast as one actor.

## Done

Every finding survives a countercheck, cites deciding evidence, explains impact and has a repair with a proving check. Report checked scope, unavailable evidence and unresolved leads. Tone is not a quality score; never claim completeness beyond the reviewed scope.
