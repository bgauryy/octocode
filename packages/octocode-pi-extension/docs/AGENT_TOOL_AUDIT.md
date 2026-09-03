# Agent tool-surface audit

Status: **accepted decision snapshot** (2026-08-25)

This document records the evidence and decisions captured on that date. It is not
the current registry contract. For current tool names, use `src/constants.ts`; for
current schemas and descriptions, inspect the built catalog. Exact character counts
below are historical measurements, not current release criteria.

## Outcome

The direct Pi palette contains **17 tools**: 16 support tools plus the guarded `bash` override.

The palette has three deliberate routing boundaries:

1. `readMedia` is read-only perception; `media` is artifact creation/transformation.
2. Awareness enters model context only when it is actionable; an unread direct message can
   trigger a bounded signal, while global dashboard counts remain user-facing.
3. `file` is the single mutation tool, with explicit `edit`, `write`, and `delete` operations.

The remaining tools each own a distinct effect or capability boundary. Scores use a 10-point agent-utility scale: routing clarity, frequency-adjusted value, safety, result quality, and contract cost. A lower score means “specialized/expensive,” not “broken.”

## Direct palette ratings

| Tool | Score | Decision | Agent value / remaining cost |
|---|---:|---|---|
| `file` | 9.7 | Keep | One guarded create/edit/delete boundary with effect discrimination, full-batch preflight, atomic writes, stale/lost-update checks, and diffs. |
| `bash` | 9.5 | Keep | Build, test, and process execution; guarded mutation paths and visible reasoning. |
| `readMedia` | 9.1 | Keep | One read-only perception route for images, video, and audio. |
| `media` | 9.0 | Keep | One write boundary for image/PDF authoring and media transformation. |
| `web` | 8.2 | Keep | Current external information and URL retrieval; repository research remains MCP-owned. |
| `chromeDebug` | 8.0 | Keep, optimize later | Unique live-page/CDP state; largest schema (5,881 chars), so it remains the top compression target. |
| `agent` | 8.7 | Keep | Parallel or isolated workers plus lifecycle control; unique concurrency boundary. |
| `callTool` | 7.7 | Keep | Reuses approved dynamic deterministic capabilities; specialized but not replaceable by `skill`. |
| `skill` | 9.0 | Keep | Progressive loading of workflow instructions avoids permanently expanding the prompt. |
| `plan` | 8.8 | Keep, optimize later | Compaction-safe execution and shared-plan/check projection; action-discriminated schema (5,748 chars). |
| `localServer` | 7.5 | Keep | Purpose-built reviewed-artifact preview with loopback and consent policy. |
| `askUser` | 8.4 | Keep | Structured decision boundary for options/forms; avoids ambiguous prose replies. |
| `memory` | 7.8 | Keep | Durable verified learning across runs; valuable only when used conditionally. |
| `lock` | 7.6 | Keep | Exceptional non-mergeable exclusivity; ordinary conflicts stay automatic. |
| `message` | 8.1 | Keep | Resolves concrete peer overlap and carries unread direct input. |
| `MCPTool` | 9.4 | Keep | Progressive gateway to the 15 Octocode research tools and configured MCP servers without registering every schema directly. |

## Awareness value/noise audit

| Feature | User value | Agent value | Decision |
|---|---:|---:|---|
| Below-editor dashboard | 9/10 | No context cost | Keep: users can see plans, tasks, peers, locks, messages, and verification debt. |
| Automatic peer registry | 7/10 | 8/10 | Keep: enables coordination without join/leave ceremony. |
| Advisory mutation presence | 6/10 | 8/10 | Keep: supplies overlap evidence automatically. |
| Mutation-time peer-lock preflight | 8/10 | 10/10 | Keep: prevents conflicting writes without requiring polling. |
| Shared projection/check receipts through `plan` | 8/10 | 9/10 | Keep: one owner for execution and truthful completion. |
| Explicit `message` | 7/10 | 8/10 | Keep: direct peer coordination has an actionable result. |
| Explicit exceptional `lock` | 6/10 | 8/10 | Keep: necessary for sensitive/non-mergeable state. |
| Durable `memory` | 7/10 | 8/10 | Keep conditionally: only verified reusable learning. |
| Generic automatic ledger-count signal | 2/10 | 3/10 | Remove: unrelated global counts distract the agent. |
| Unread direct-message count | 6/10 | 8/10 | Keep narrowly: it is targeted, bounded, and routes to `message` inbox. |
| Awareness skill/CLI diagnostics | 5/10 | 8/10 when needed | Keep out of the default palette; load only for overlap, recovery, or deeper diagnosis. |

## Captured contract-efficiency baseline

Measured from the built extension by summing every direct tool's top-level description and serialized parameter schema:

| Metric | 2026-08-25 baseline |
|---|---:|
| Direct tools | 16 |
| Contract characters | 40,377 |
| Approximate tokens (4 chars/token) | 10,095 |

All public descriptions now come from one curated catalog. Registration recursively normalizes whitespace and caps every schema description at 180 characters. Tests enforce catalog coverage, a 360-character top-level cap, the 180-character schema cap, and a 45,000-character whole-palette budget.

## Recorded follow-up priority

No additional tool should be removed now. The next efficiency work should compress schemas rather than merge distinct effects, in this order:

1. `chromeDebug` (5,881 contract chars)
2. `plan` (5,748)
3. `agent` (3,732)
4. `askUser` (3,592)

Any future removal must first prove that its effect and result can be owned by another existing boundary without creating an overloaded “do anything” tool.
