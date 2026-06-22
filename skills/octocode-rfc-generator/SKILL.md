---
name: octocode-rfc-generator
description: "Use for RFCs, design docs, architecture proposals, migration plans, implementation plans, and research-backed technical decisions before coding. Leverages Octocode local/GitHub/npm/binary tools via MCP or CLI to gather evidence, compare alternatives, map blast radius, and produce a validated RFC with a practical implementation plan."
---

# Octocode RFC Generator

Use this skill when a change needs **thinking before coding**: architecture choices, migrations, cross-package changes, risky refactors, implementation plans, or formal RFC/design docs. The output is evidence-backed and actionable, not a brainstorm.

Core flow:

```text
UNDERSTAND → RESEARCH → COMPARE OPTIONS → WRITE RFC / PLAN → VALIDATE → DELIVER
```

Default output location when saving is approved: `.octocode/rfc/RFC-{meaningful-name}.md`.

## 1. Pick mode

| User asks for | Mode | Output |
|---|---|---|
| “write RFC”, “design doc”, “proposal”, “architecture decision” | RFC | Full RFC with alternatives, rationale, risks, implementation plan |
| “plan this work”, “research and build”, “implementation plan” | Plan | Evidence-backed implementation plan; RFC sections included only when useful |
| “compare approaches”, “should we use X or Y” | Decision | Options matrix + recommendation + adoption/rollback notes |
| “migration plan” | Migration | Current state, target state, compatibility, rollout, rollback, phases |
| “validate this RFC/design” | Validation | Claim-by-claim verdict with evidence and gaps |

If the task is a trivial one-file edit with no design choice, say an RFC is unnecessary and suggest using `octocode-engineer` directly.

## 2. Research with Octocode

Pick MCP if available; otherwise use the CLI. Do not guess facts that tools can verify.

### Local codebase evidence

Use when the current repo matters.

| Need | MCP | CLI |
|---|---|---|
| Map structure | `localViewStructure` | `octocode ls` |
| Find files | `localFindFiles` | `octocode find` |
| Search code | `localSearchCode` | `octocode grep` |
| Read exact code | `localGetFileContent` | `octocode cat` |
| AST shape proof | `localSearchCode(mode:"structural")` | `octocode grep <path> --pattern '<shape>'` |
| Symbols / LSP | `lspGetSemantics` | `octocode ls --symbols` / `octocode lsp` |

Local flow:

```text
ls → find/grep → symbols → matchString/line range → AST/LSP → cited current-state evidence
```

### External evidence

Use for prior art, package choices, cross-repo comparison, and history.

| Need | MCP | CLI |
|---|---|---|
| Package → repo | `npmSearch` | `octocode pkg` |
| Discover repos | `ghSearchRepos` | `octocode repo` |
| Map repo | `ghViewRepoStructure` | `octocode ls owner/repo` |
| Search GitHub | `ghSearchCode` | `octocode grep kw owner/repo` |
| Read GitHub file | `ghGetFileContent` | `octocode cat owner/repo/path` |
| PR/commit history | `ghHistoryResearch` | `octocode pr` / `octocode history` |
| Clone for deep proof | `ghCloneRepo` | `octocode clone` |

External flow:

```text
pkg/repo → ghViewRepoStructure → ghSearchCode path/content discovery → ghGetFileContent proof → history/PR rationale
```

Clone and switch to local tools when analysis spans several files or needs AST/LSP.

### Binary/archive evidence

Use when the source is packaged or compiled.

| Need | MCP | CLI |
|---|---|---|
| Identify file type | `localBinaryInspect(mode:"identify")` | `octocode binary --identify` |
| List archive entries | `localBinaryInspect(mode:"list")` | `octocode binary --list` |
| Extract one entry | `localBinaryInspect(mode:"extract")` | `octocode binary --extract` |
| Decompress stream | `localBinaryInspect(mode:"decompress")` | `octocode binary --decompress` |
| Inspect strings | `localBinaryInspect(mode:"strings")` | `octocode binary --strings` |
| Unpack archive | `localBinaryInspect(mode:"unpack")` | `octocode unzip` |

Flow: identify/list → extract one entry or unpack all → run local tools on the returned `localPath`.

## 3. Understand

Capture this before research gets broad:

- Problem in one or two sentences.
- Why this needs a decision/plan.
- Affected users, packages, APIs, teams, or workflows.
- Constraints: compatibility, performance, security, rollout, tech stack.
- What “do nothing” costs.
- What evidence is needed to decide.

Ask if the problem or desired output mode is unclear.

## 4. Research plan

Run only the tracks that matter.

| Scenario | Research tracks |
|---|---|
| Existing-system change | Local current state + local blast radius; external prior art if options are unclear |
| Greenfield choice | External prior art + package/repo comparison; local constraints if repo exists |
| Migration | Local current state + contracts/data flows + external migration examples |
| Library/package adoption | npm/package metadata + repo source + local integration points |
| Refactor plan | Local structure + LSP references/callers + AST duplication/smell checks |
| RFC validation | Map each claim to local/external evidence; mark confirmed/likely/uncertain |

Evidence rules:
- Local claims need `file:line`.
- External code claims need GitHub file path/line or PR/commit link.
- Snippets are leads; use `matchString`, line ranges, AST, LSP, or history before citing.
- Key recommendations need at least one supporting source and one counterpoint or rejected alternative.

## 5. Compare options

Always include at least two alternatives unless the user explicitly asks for a single implementation plan.

Useful alternatives:
- Do nothing / defer.
- Minimal patch.
- Incremental migration.
- Full redesign.
- Adopt package/library.
- Build in-house.
- Hybrid/phased rollout.

Compare on:
- Fit with current architecture.
- Blast radius.
- Compatibility and migration cost.
- Operational risk.
- Performance/security/data implications.
- Maintenance and ownership.
- Reversibility.

## 6. Write the RFC or plan

Use `references/rfc-template.md` for full RFCs; do not duplicate the whole template in the prompt. For implementation plans, include only the sections needed for the work:

```markdown
# Plan: <title>

## Goal
## Evidence Summary
## Current State
## Proposed Approach
## Alternatives Considered
## Step-by-Step Implementation
## Files / APIs / Contracts Touched
## Test and Verification Plan
## Rollout / Migration / Rollback
## Risks and Open Questions
```

Implementation steps should be ordered by dependency, not preference. Avoid time estimates.

## 7. Validate

Before delivering, check:

- Problem and motivation are specific.
- Current state has real evidence.
- Alternatives are fairly compared.
- Recommendation follows from evidence.
- Drawbacks and migration costs are explicit.
- Blast radius is mapped for shared symbols/contracts.
- Risks have mitigations or open questions.
- Implementation plan is actionable and verifiable.
- No claim relies on “common practice” without explaining why it applies here.

Reasoning traps:
- First-option bias: search for evidence against the preferred approach.
- False dichotomy: consider hybrids/phased plans.
- Local-vs-external conflict: local constraints usually win; document the tradeoff.
- Metrics claims: use external tools or mark as approximation.

## 8. Deliver

Start with a concise summary:

```text
Decision: <recommendation>
Why: <1-2 evidence-backed reasons>
Alternatives: <count and names>
Risk: <low|medium|high + why>
Next step: <one action>
```

Then ask whether to save the full RFC/plan.

- If yes: save to `.octocode/rfc/RFC-{meaningful-name}.md`.
- If no: keep it in chat.
- If user wants implementation: hand off to the agent’s normal engineering/edit workflow using the implementation plan.

## 9. Recovery

| Situation | Move |
|---|---|
| Local search empty | broaden search, inspect structure, try symbols/AST variants |
| GitHub search empty | use repo structure/path search, known files, or clone |
| No external prior art | say so; rely on local constraints and unresolved questions |
| Evidence conflicts | present conflict and decision rule |
| Scope too broad | split into multiple RFCs or phases |
| Two attempts fail | summarize what is known and ask for direction |
