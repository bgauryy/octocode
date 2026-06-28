# Review Workflow — PR, Local Changes, and History

Use this when the user asks to review a pull request, review local/staged changes, check whether a PR is safe to merge, or inspect file/repo history. The main engineering contract, gates, and artifact rules remain in `../SKILL.md`.

## 1. Target detection

| User input | Mode | Primary tools |
|---|---|---|
| PR URL, `owner/repo#N`, “review PR N” | Remote PR | `npx octocode search --target pullRequests`, selected patches/comments, optional clone/cache for AST/LSP |
| “review my changes”, “staged”, “local diff” | Local changes | shell `git status`/`git diff`, then `npx octocode search`, `search --tree`, `search --content-view`, and `search --op` |
| “who changed this file”, “file history”, “commit archaeology” | History | `npx octocode search <owner/repo[/path]> --target commits` → PR deep read |
| Specific local file path | Local file scope | `npx octocode search <file-or-parent> --tree` / `npx octocode search <query> <path> --search path` → `npx octocode search <file> --content-view ...` → `npx octocode search <file> --op ...` |
| Ambiguous | Ask | “Review a PR, local changes, or commit history?” |

## 2. Guidelines gateway

Before reviewing, check for project rules:

- Local: `npx octocode search <name> . --search path` for `.octocode/pr-guidelines.md`, `.octocode/context/context.md`, `CONTRIBUTING.md`, `AGENTS.md`.
- Remote: `npx octocode search "pr-guidelines|CONTRIBUTING|AGENTS" <owner/repo> --view discovery` or `npx octocode search <owner/repo> --tree` for likely docs.
- Read found files with `npx octocode search <file> --content-view exact` or `npx octocode search <file> --match-string ... --content-view exact`.
- Ask the user if they have additional review guidelines or focus areas.

Precedence:
1. User-provided guidelines.
2. `.octocode/pr-guidelines.md`.
3. `.octocode/context/context.md`, `CONTRIBUTING.md`, `AGENTS.md`.
4. Default review domains in `checklist-review-domains.md`.

## 3. Remote PR workflow

```text
npx octocode search <owner/repo#N> --target pullRequests --json
→ npx octocode search <owner/repo#N> --target pullRequests --comments --json
→ classify files: HIGH risk (auth/API/data/logic/contracts) vs LOW risk (docs/CSS/config)
→ npx octocode search <owner/repo#N> --target pullRequests --patches --file <highRiskFile> --json
→ paginate patches/comments from JSON pagination/hints; never compute offsets
→ npx octocode search <owner/repo/path> --content-view symbols / --match-string ... --content-view exact for current source
→ clone/cache repo/subtree only if AST/LSP proof is needed
→ analyze changed lines plus directly affected context
→ dedupe against existing PR comments before reporting
```

Rules:
- Fetch metadata + changed files before patches.
- Use `--patches --file <path>` first; use `--deep` only for small PRs or when comments/commits/reviews are all needed.
- Existing PR comments are a dedupe guard: do not repeat them; verify unresolved comments were fixed or re-flag as unresolved.
- Large PR (>500 lines or >15 files): recommend splitting or focus review on high-risk areas.
- If `search --target pullRequests` cannot express the exact selector or pagination lane, run `npx octocode tools ghHistoryResearch --scheme` and use raw `ghHistoryResearch` with selected `content`.

Suggested CLI cadence:

```bash
npx octocode search owner/repo#1234 --target pullRequests --json
npx octocode search owner/repo#1234 --target pullRequests --comments --json
npx octocode search owner/repo#1234 --target pullRequests --patches --file packages/app/src/auth.ts --json
npx octocode search owner/repo/packages/app/src/auth.ts --match-string "changedSymbol" --content-view exact --json
```

Use `npx octocode clone owner/repo/path` or `npx octocode cache fetch owner/repo path --depth tree` after triage when the review needs structural search, LSP, package context, or repeated local searches. Continue with local `search --tree`, `search`, `search --content-view`, and `search --op` against the returned `localPath`.

## 4. Local changes workflow

```text
git status
→ git diff --staged / git diff / git diff HEAD based on requested scope
→ git branch --show-current + git log --oneline -10
→ npx octocode search <changed-parent-dir> --tree
→ npx octocode search <file> --match-string <changedFunctionName> --content-view exact
→ npx octocode search for exact symbols → npx octocode search <file> --op references/callers/callHierarchy
→ npx octocode search --pattern/--rule --lang <language> for smell/security/perf patterns
→ report only changed-code issues plus direct blast-radius risks
```

Rules:
- Shell `git` is allowed for diff/status/log. Use octocode local tools for code reading/searching.
- If no changes are detected, stop and tell the user.
- If diff is too large, ask to scope to staged/unstaged/specific files.

## 5. Flow impact recipes

- Changed function signature → `npx octocode search <symbol> <path> --json` → `npx octocode search <file> --op callers --symbol <symbol> --line <lineHint> --format compact --json`.
- Changed type/interface → `npx octocode search <file> --op references --symbol <typeName> --line <lineHint> --json`; use raw `lspGetSemantics` for `groupByFile` if needed.
- Changed behavior with outgoing dependencies → `npx octocode search <file> --op callees --symbol <symbol> --line <lineHint> --json`.
- High-risk flow → `npx octocode search <file> --op callHierarchy --symbol <symbol> --line <lineHint> --depth 2 --format compact --json`.
- Remote-only PR without clone → `npx octocode search` for imports/callers → `npx octocode search <file> --match-string ... --content-view exact` for proof; mark confidence lower than local LSP.

## 6. Checkpoint and finalization

After context fetch, present a short checkpoint:

```text
Scope: <PR/local/history>, <files/lines>, risk areas
High-risk files: <list>
Existing comments/guidelines: <summary>
Question: Full review or focus on specific areas?
```

Before final report:
- Deduplicate by root cause and file line.
- Keep only HIGH/MED confidence findings; mark uncertainty explicitly.
- Cap at ~5–7 key issues.
- Every finding needs exact `file:line`, severity, confidence, and actionable fix.
- Never label findings as `#1`, `#2`, `#N`; GitHub auto-links those. Use `1.` or `[SEC-1]`.
- Include a merge recommendation: `safe to merge`, `fix before merge`, `split PR`, or `needs more context`.
