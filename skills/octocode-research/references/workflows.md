# Workflows

Use this to pick the efficient Octocode route for local research, external research, root cause, and PR/local review. Read `algorithm.md` first for the router and evidence grades; read `octocode.md` when transport or command syntax is unclear.

## Common Spine

```text
scope -> surface plan -> cheap map -> anchor -> exact read -> stronger proof -> answer/patch/review
```

Start with the corpus and skipped surfaces: local path, owner/repo/ref, PR number, package/version, artifact, history window, and why each surface is active or skipped.
Prefer cheap orientation before deep reads. Promote claims only after exact evidence plus at least one stronger lane: AST shape, LSP identity, history/PR intent, artifact metadata, docs/specs, or tests.

## Minimal Reads

Use the lightest reference stack that can answer the task:

| Task | Load |
|---|---|
| Small factual/code question | `algorithm.md`; add `octocode.md` only if transport is unclear |
| Local or external route choice | `algorithm.md` + `workflows.md` |
| Bug, root cause, review, or code finding | `algorithm.md` + `workflows.md` + `code-research.md` |
| Implementation/change | `algorithm.md` + `workflows.md` + `code-research.md`; add `loop-mode.md` after failed verification |
| Long/contested decision | `algorithm.md` + `long-research.md`; add `github-landscape.md` only for repo ecosystem ranking |

Default receipt for handoffs and subagents:

```text
mode | scope | active/skipped surfaces | claims with evidence/confidence/gaps | verification | next step
```

## Local Research

Use when the running repo, local checkout, local artifact, or installed dependency is the truth.

```text
localViewStructure / localFindFiles
-> localSearchCode for terms, identifiers, or changed anchors
-> localGetFileContent(symbols or matchString)
-> lspGetSemantics for definition, references, callers, callees, hover
-> localSearchCode structural/OQL when shape, reachability, or drift matters
```

Local-first defaults:
- For package behavior, inspect `node_modules/<pkg>` before GitHub; it is the version that runs.
- For impact claims, diff broad text hits against LSP results before saying "unused", "only", or "safe".
- For edits, find a local pattern first, patch the smallest scope, then run the targeted verification.

Use external surfaces only when they answer something local cannot: upstream intent, fixes in newer versions, PR/commit history, source repo tests, or ecosystem alternatives.

## External Research

Use when the corpus is a remote repo, PR, package, prior-art question, or an upstream dependency not present locally.

```text
npmSearch / ghSearchRepos for discovery
-> ghViewRepoStructure for orientation
-> ghSearchCode for anchors
-> ghGetFileContent(matchString or symbols) for exact proof
-> ghHistoryResearch for PR/commit intent
-> materialize when AST, LSP, negative proof, repeated reads, or local tests matter
```

External-proof rules:
- GitHub search zeros are provider evidence, not absence. Verify path/ref, try synonyms, inspect structure, then materialize before strong negative claims.
- Track `resolvedBranch`/ref and cite it. A fallback branch changes what was actually researched.
- Packages: use npm/package metadata to find the source repo, but use exact code/docs/tests before recommending reuse.
- Materialize after the third read into one remote area, or earlier when structural search, LSP, many-file search, binary inspection, or exact absence matters.

## Root Cause

Use when the user asks why behavior changed, a test failed, an error appears, or a bug exists.

```text
capture reproduction when available: failing command, log, input, stack frame, endpoint, or changed behavior
-> symptom anchor: error string, failing test, endpoint, file, stack frame, or changed behavior
-> two hypotheses: likely cause + plausible alternate
-> trace entry -> transformation -> failing/changed contract
-> exact reads around each boundary
-> AST/LSP/history/tests to disconfirm one hypothesis
-> mechanism + trigger + fix surface
```

Keep the root cause answer tight:

```text
Root cause: <mechanism and trigger>
Evidence: <path:line / PR / command output>
Why now: <change, input, dependency, config, or data condition>
Fix: <smallest safe repair or decision needed>
Verification: <test/build/search/history check run or still needed>
```

Do not stop at "probably X" when a cheap disconfirming read exists. If both hypotheses survive, ask for the missing runtime input/log/config instead of pretending certainty.

## Change Mode

Use when the user asks to implement, refactor, migrate, or patch after evidence gathering.

```text
current contract + invariants
-> blast radius: callers, references, imports, tests, configs
-> existing local pattern to copy
-> patch boundary: smallest files/symbols that solve the claim
-> verify: targeted test/build/typecheck/lint/smoke or exact read when no runtime check exists
-> if failed: read the failing path, update the ledger, patch only the cause, or report blocked
```

Change rules:
- Ask before public contracts, cross-package edits, deletes/renames, or many consumers.
- Do not mix opportunistic cleanup with the requested patch.
- Final answer states patch scope, verification that ran, remaining gaps, and confidence.

## PR Or Local Review

Use for `review PR`, `safe to merge`, `review my changes`, staged/unstaged diffs, and local file-scope review. Review changed code and direct blast radius; avoid style-only, unchanged-code, generated/vendor, and already-commented findings unless unresolved.

Target route:
- Remote PR: `ghHistoryResearch` metadata + changed files first, then existing comments, then selected patches for high-risk files. Use full PR fetch only for small PRs or when the user asks. Follow patch/comment pagination before concluding.
- Local diff: shell `git status`/`git diff` selects changed regions; Octocode local tools read/search code, map structure, and prove symbol impact. Respect staged/unstaged/file scope from the user.
- Remote-only without clone: use `ghSearchCode`/`ghGetFileContent` for consumers and mark confidence lower than local LSP unless materialized.

Review loop:
1. Load guidelines/context if present: user rules, `.octocode/pr-guidelines.md`, `CONTRIBUTING.md`, `AGENTS.md`, or repo context.
2. Summarize scope, files/lines, risk areas, existing comments, and review mode.
3. Ask for focus when the review is ambiguous, medium/high risk, >5 files, guidelines conflict, or the user asked a safe-to-merge/product decision. For tiny low-risk reviews, proceed with stated assumptions.
4. For each changed public/high-risk symbol, anchor with search, then use LSP callers/references/callees or remote consumer search.
5. Check domains in priority order: Security, Bug, Flow Impact, Architecture, Performance, Error Handling, Quality, Guidelines/Duplicates.
6. Verify existing PR comments were fixed or keep them as unresolved context; do not duplicate them as new findings.
7. Dedupe by root cause, drop low-confidence nits, and cap the final report to the highest-impact findings.

Each review finding needs:

```text
[DOMAIN-1] title
Severity: HIGH|MED|LOW
Confidence: confirmed|likely|uncertain
Location: path:line
Evidence: exact read + proof lane
Impact: caller/user/data/contract consequence
Fix: minimal code direction or diff
```

Avoid `#1`/`#2` finding labels in GitHub contexts because they auto-link. If no findings survive, say so and name residual gaps: tests not run, LSP unavailable, remote-only proof, missing runtime input, or intentionally skipped surfaces.
