# astSearch questions

Use the [shared question protocol](README.md), [CLI/MCP checks](../../CHECKS.md#every-tool-every-time) and [expert alternatives](../../COMPARISONS.md). Assign `FIXTURE` before either arm starts; the 2026-11-14 round pins `campaigns/20.2.0-2026-11-14-tool-ratings/local/fixture` with the adjacent `fixture-manifest.json`. Expected sets and source oracles stay in the campaign, outside these prompts.

## AS-01

Find free calls of `runTask` in `FIXTURE/src/calls.ts`, excluding definitions, comments, string contents and receiver calls. Compare selected locations and source snippets with the equivalent ast-grep pattern; a lexical rg result is only a decoy baseline.

## AS-02

Find `runTask` calls contained in the `outer` declaration, including the receiver-call form if the rule selects it. Use a structural kind/inside or has rule and compare the identical selection with ast-grep. Record rule semantics before running.

## AS-03

List symbols from `FIXTURE/src` with small row and response windows and complete all continuations. Cross-check declared source identities with the frozen source and symbols view of localFetch. Keep source-confirmed symbol scope distinct from reference identity.

## AS-04

Search `FIXTURE/src/receiver.rs` with the bare free-call pattern `send($$$A)` where receiver calls may be a distinct shape. Assess the empty-result coverage and any executable lead, then follow the lead to settle the requested call shape. Compare both forms to ast-grep.

## AS-05

Request the syntax tree around line 5 of `FIXTURE/src/calls.ts`, preserving named-node ranges; compare the corresponding ast-grep inspection. Then submit a match operation without its required `rule`, assessing correction guidance and parity.

## AS-06

Repeat AS-01, then search a disposable copy before and after adding a free call. Establish fresh source locations without changing the frozen fixture.

## AS-07

Find free `runTask` calls in `FIXTURE/src/calls.ts` as files and countMatches views, then ask for an exact symbol filter selecting `outer`. Compare the same counts and symbol declarations with source and ast-grep, separating occurrences from matching files.
