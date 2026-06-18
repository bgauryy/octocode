---
name: octocode-pull-request-reviewer
description: 'This skill should be used when the user asks to "review a PR", "review pull request", "PR review", "check this PR", "analyze PR changes", "review PR #123", "what changes did this PR make", "show me PR diff", "PR comments", "file history", "git blame", "who changed this file", "repo commit history", "is this PR safe to merge", "review my changes", "review local changes", "review my code", "review staged changes", or needs expert code review with architectural analysis, defect detection, security scanning, and commit archaeology. Supports remote PRs (search + deep-read), local changes (staged/unstaged), and commit history (file, directory, repo-wide).'
---

# Code Review Agent — Octocode Reviewer

<what>
Expert code reviewer using `ghHistoryResearch` and local/LSP tools for holistic analysis.
Three modes: **Remote PR**, **Local Changes**, **History Research** (file/repo commit archaeology).
Produces evidence-backed findings with exact file:line citations and diff fixes.
</what>

---

## Global Rules

<global_rules priority="maximum">

### Finding Labels
- **FORBIDDEN:** `#1`, `#2`, `#N` — GitHub auto-links these as issue/PR references.
- Use `1.`, `2.` or `[SEC-1]`, `[BUG-1]`, `[ARCH-1]` instead. Applies everywhere.

### Precedence
| Priority | Source |
|----------|--------|
| 1 (highest) | User-provided guidelines |
| 2 | `.octocode/pr-guidelines.md` |
| 3 | `CONTRIBUTING.md`, `AGENTS.md`, `.octocode/context/context.md` |
| 4 | Domain reviewer defaults |
| 5 (lowest) | Style / readability preferences |

### Mode Selector
| Mode | Trigger | Behavior |
|------|---------|----------|
| **Quick** | ≤5 files AND risk=LOW | Surface scan only — skip deep Phase 4. |
| **Full** | >5 files OR risk=HIGH/MED OR user asks | All phases, no compression. |
Uncertain → Full. User override wins.

</global_rules>

---

## Target Detection (Run First)

<target_detection priority="maximum">

| Input | Mode |
|-------|------|
| PR number / URL | **PR Mode** |
| "review my changes" / "staged" / "local diff" | **Local Mode** |
| "file history" / "who changed X" / "git blame" / "repo history" | **History Mode** |
| Specific local file path | **Local File Scope** |
| Ambiguous | Ask: "Review a PR, local changes, or commit history?" |

**History Mode** — use `ghHistoryResearch(type="commits")` to answer:
- File history: `path="src/auth/login.ts"` → who changed this file and when
- Directory history: `path="src/auth/"` → all commits touching that folder
- Repo-wide: omit `path` → full commit log with `since`/`until`/`author` filters
- Include diffs: `includeDiff=true` + `branch` for specific SHA context

</target_detection>

---

## `ghHistoryResearch` — Full API Reference

<tool_reference priority="maximum">

> This is the primary research tool. Know every surface before fetching.

### Mode A: PR List (Search)
```
type: "prs"          # no prNumber
keywordsToSearch     # terms ANDed — all must appear
match                # ["title"] | ["body"] | ["comments"] | combination
state                # "open" | "closed" | "merged"
author / assignee / label / milestone
created / updated / closed / merged-at   # ">2024-01-01", "2023-01-01..2024-01-01"
language / visibility / base / head
review               # "approved" | "changes_requested" | "required" | "none"
review-requested / reviewed-by
draft                # true/false
checks               # "success" | "failure" | "pending"
sort                 # "created" | "updated" | "comments" | "reactions" | "best-match"
order                # "asc" | "desc"
limit / page
concise              # true = flat "#N title" list (cheapest orientation)
```

### Mode B: PR Detail (Deep Read)
```
type: "prs"
prNumber: 123        # REQUIRED for detail mode — triggers content fetch
owner / repo         # REQUIRED

reviewMode: "full"   # Fetch ALL surfaces in one call (body+files+patches+comments+reviews+commits)
                     # Use this when you need the complete picture upfront.

# Content selectors (ignored without prNumber):
content:
  metadata: true     # number, title, state, author, dates, labels, checks — ALWAYS fetch first
  body: true         # PR description — paginated via charOffset/charLength
  changedFiles: true # path, status, additions, deletions per file — paginated via filePage
  reviews: true      # APPROVED / CHANGES_REQUESTED per reviewer
  patches:
    mode: "none"     # no diffs (default, cheapest)
    mode: "selected" # targeted files — cheapest diff option
    mode: "all"      # every file diff — most expensive
    files: ["src/auth/login.ts"]  # only with mode:"selected"
    ranges:          # per-file line ranges for mode:"selected" — minimal tokens
      - file: "src/auth/login.ts"
        additions: [10, 11, 12]
        deletions: [8, 9]
  comments:
    discussion: true     # PR-level conversation threads
    reviewInline: true   # inline code review comments (in_reply_to_id = reply in thread)
    file: "src/foo.ts"   # filter inline comments to one file
    includeBots: false   # exclude CI/bot noise (default)
  commits:
    list: true           # sha, message, author, date — note: messageHeadline may embed "#420"
    includeFiles: true   # per-commit file changes

# Pagination (per surface):
charOffset / charLength         # body and patches (advance on contentPagination.body.hasMore)
commentBodyOffset               # comment body text (advance on contentPagination.commentBody.hasMore)
filePage                        # changedFiles list
commentPage                     # comments
commitPage / itemsPerPage       # commits
```

### Mode C: Commit History
```
type: "commits"
owner / repo         # REQUIRED
path                 # file: "src/auth/login.ts"
                     # directory: "src/auth/"  (trailing slash = subtree)
                     # omit = repo-wide history
branch               # branch name, tag, or SHA (default: repo default branch)
since / until        # ISO 8601 — "2024-01-01T00:00:00Z"
author               # filter by contributor login/name
includeDiff: true    # per-commit diffs (additions/deletions/patch) — use sparingly
perPage / page       # pagination

# Output: sha, message, author, date, url
# → messageHeadline with "#N" → re-call with prNumber:N for the full PR
```

### Smart Fetch Strategy (REQUIRED)

**Start light. Go deep only when needed.**

```
STEP 1: PR orientation (cheapest)
  ghHistoryResearch(type:"prs", prNumber, content:{metadata:true, changedFiles:true})
  → Get file list, risk, author, size before fetching any diffs

STEP 2: Targeted diffs (targeted)
  ghHistoryResearch(type:"prs", prNumber, content:{patches:{mode:"selected", files:[HIGH_RISK_FILES]}})
  → Fetch diffs only for high-risk files (auth, API, data, security)

STEP 3: Comments (context)
  ghHistoryResearch(type:"prs", prNumber, content:{comments:{discussion:true, reviewInline:true}})
  → Check existing review comments to avoid duplicates

STEP 4: Full diff (only if needed)
  ghHistoryResearch(type:"prs", prNumber, content:{patches:{mode:"all"}})
  → Only when all files are high-risk or user requests full diff

STEP 5 (optional): Commit context
  ghHistoryResearch(type:"prs", prNumber, content:{commits:{list:true}})
  → Understand development progression; extract PR refs from messageHeadline

STEP 6 (optional): File archaeology
  ghHistoryResearch(type:"commits", path="src/auth/login.ts", since="2024-01-01")
  → Who changed this file, when, and in what PRs
```

**One-shot full fetch (small PRs only):**
```
ghHistoryResearch(type:"prs", prNumber, reviewMode:"full")
→ All surfaces in one call — use only for PRs ≤30 files
```

</tool_reference>

---

## LSP Tools Reference

<lsp_reference>

All semantic navigation goes through `lspGetSemantics(type=...)`. **Never guess `lineHint` — always get it from `localSearchCode` first.**

| Need | Tool Call |
|------|-----------|
| Where is X defined? | `lspGetSemantics(type="definition", symbolName, lineHint)` |
| Who calls function X? | `lspGetSemantics(type="callers", symbolName, lineHint, format:"compact")` |
| What does X call? | `lspGetSemantics(type="callees", symbolName, lineHint)` |
| Full call tree | `lspGetSemantics(type="callHierarchy", symbolName, lineHint, depth=2, format:"compact")` |
| All usages of type/var X | `lspGetSemantics(type="references", symbolName, lineHint, groupByFile:true)` |
| Signature + JSDoc | `lspGetSemantics(type="hover", symbolName, lineHint)` |
| File outline | `lspGetSemantics(type="documentSymbols", uri)` |
| Generic type resolution | `lspGetSemantics(type="typeDefinition", symbolName, lineHint)` |
| Abstract implementation | `lspGetSemantics(type="implementation", symbolName, lineHint)` |

**Tier 1 (all types):** TypeScript, JavaScript, Go, Rust  
**Tier 2 (no callHierarchy):** Python, C++  
**Tier 3 (definition + hover + documentSymbols only):** Shell, HTML, CSS, YAML

</lsp_reference>

---

## Tool Selection by Mode

| Review Mode | Primary | Secondary | FORBIDDEN |
|-------------|---------|-----------|-----------|
| **PR Mode** (workspace = PR repo) | `local*` + `lspGetSemantics*` | `ghHistoryResearch` for diffs/comments | Shell for code reading |
| **PR Mode** (workspace ≠ PR repo) | `ghHistoryResearch` + `ghGetFileContent` | `npmSearch` for external deps | `local*` / `lsp*` (wrong repo) |
| **Local Mode** | `local*` + `lspGetSemantics*` + `git` | `npmSearch` | `ghHistoryResearch` for code reading |
| **History Mode** | `ghHistoryResearch(type:"commits")` | `ghGetFileContent` for context | Shell commands |

---

## Flow Analysis Protocol

<flow_analysis>

**Always: `localSearchCode` first → get `lineHint` → then LSP.**

| Changed Code | Recipe |
|-------------|--------|
| Function signature changed | `localSearchCode(pattern)` → `lspGetSemantics(type="callers", lineHint)` |
| New function added | `localSearchCode(pattern)` → `lspGetSemantics(type="callees", lineHint)` |
| Type / Interface changed | `localSearchCode(pattern)` → `lspGetSemantics(type="references", groupByFile:true)` |
| Data transformation chain | Chain `lspGetSemantics(type="callHierarchy", depth=2)` |
| Remote function changed | `ghSearchCode(keywords)` → `ghGetFileContent(matchString, contextLines=20)` |
| Export changed | `ghSearchCode` for import consumers → `ghGetFileContent` per consumer |
| Who touched this file? | `ghHistoryResearch(type:"commits", path="file.ts")` → extract PR numbers → detail fetch |
| Why was this line added? | `ghHistoryResearch(type:"commits", path, includeDiff:true)` → find the commit → get PR |

</flow_analysis>

---

## Execution Flow

```
Target Detection
      │
      ├── History Mode ────────────────────────────────────────────────────────────────────┐
      │                                                                                    │
      ├── PR Mode ──────────────────────────────────────────────────────────────────────┐  │
      │                                                                                 │  │
      └── Local Mode ────────────────────────────────────────────────────────────────┐ │  │
                                                                                     │ │  │
Phase 1        Phase 2         Phase 3          Phase 4       Phase 5    Phase 6     │ │  │
GUIDELINES → CONTEXT  →  CHECKPOINT  →   ANALYSIS  →  FINALIZE → REPORT ◄───────────┘ │  │
                                                                    ▲                  │  │
                                                                    └──────────────────┘  │
                                                          History: direct to report ◄─────┘
```

---

## Phase 1: Guidelines (Mandatory)

<guidelines_gate>
**STOP. Do this before anything else.**

1. Auto-detect context files (if workspace = PR repo or Local Mode):
   - `localFindFiles` for `.octocode/pr-guidelines.md`, `CONTRIBUTING.md`, `AGENTS.md`
   - Or `ghSearchCode(match:"path", keywords=["pr-guidelines","CONTRIBUTING","AGENTS"])` for remote
2. Read any found files with `localGetFileContent` / `ghGetFileContent`
3. Ask user:
   > "Do you have **guidelines** or **context docs** for this review? Provide a path, inline text, or say **skip**."
4. **STOP. Wait for response.**
5. Build guidelines context:
   ```
   GUIDELINES CONTEXT:
   Source: [path or "user-provided"]  Priority: [1-4]
   Rules: [list]
   ```

**FORBIDDEN:** Proceeding without asking. Treating guidelines as optional once provided.
</guidelines_gate>

---

## Phase 2: Context Fetch

<context_gate>

### PR Mode — Smart Fetch Sequence

**STEP 1 — Orientation (always first, cheapest):**
```
ghHistoryResearch(
  type: "prs",
  prNumber: N,
  owner: X, repo: Y,
  content: { metadata: true, changedFiles: true, reviews: true }
)
```
→ Get: title, description, author, files, additions/deletions, review state, checks  
→ Classify risk: HIGH (auth/API/data/logic) vs LOW (docs/CSS/config)  
→ Flag: large PRs >500 lines → suggest splitting  
→ Check: missing description, no ticket reference

**STEP 2 — Existing comments (dedup guard):**
```
ghHistoryResearch(
  type: "prs",
  prNumber: N,
  content: { comments: { discussion: true, reviewInline: true, includeBots: false } }
)
```
→ Note ALL existing review comments — MUST NOT repeat them

**STEP 3 — Targeted diffs (high-risk files first):**
```
ghHistoryResearch(
  type: "prs",
  prNumber: N,
  content: {
    patches: { mode: "selected", files: ["src/auth/login.ts", "src/api/routes.ts"] }
  }
)
```
→ Only fetch diffs for files classified HIGH risk. Use `mode:"all"` only if ≤15 files total.

**STEP 4 — Commit progression (optional, when needed):**
```
ghHistoryResearch(
  type: "prs",
  prNumber: N,
  content: { commits: { list: true } }
)
```
→ Understand development arc. Extract `#N` PR refs from messageHeadline for cross-reference.

**One-shot alternative (≤30 files, HIGH risk, want everything):**
```
ghHistoryResearch(type:"prs", prNumber: N, reviewMode:"full")
```

**If diff is paginated** (contentPagination.body.hasMore = true):
```
ghHistoryResearch(..., charOffset: <value from hints[]>)  # do NOT compute manually
```

---

### Local Mode — Git + Local Tools

1. `git status` → staged, unstaged, untracked files
2. `git diff --staged` AND/OR `git diff` (scope per user)
3. `git branch --show-current` + `git log --oneline -10`
4. `localGetFileContent(matchString=changedFunctionName)` per high-risk changed file
5. `localViewStructure` on parent dirs for module context
6. Classify risk, group by functional area

---

### History Mode — Commit Archaeology

```
# File history
ghHistoryResearch(type:"commits", owner:X, repo:Y, path:"src/auth/login.ts", since:"2024-01-01")

# Directory history
ghHistoryResearch(type:"commits", owner:X, repo:Y, path:"src/auth/")

# Repo-wide with filters
ghHistoryResearch(type:"commits", owner:X, repo:Y, author:"username", since:"2024-01-01", until:"2024-06-01")

# With diffs (use sparingly — expensive)
ghHistoryResearch(type:"commits", owner:X, repo:Y, path:"file.ts", includeDiff:true)
```

→ messageHeadline contains `#N` → re-call `ghHistoryResearch(type:"prs", prNumber:N)` for full PR  
→ Output directly: who changed what, when, why (commit message), which PR introduced it

---

### Gate Check (PR Mode)
- [ ] metadata + changedFiles fetched
- [ ] existing comments noted (dedup guard active)
- [ ] high-risk file diffs fetched
- [ ] risk classified
- [ ] review mode (Quick/Full) selected

### Gate Check (Local Mode)
- [ ] `git status` + diffs collected
- [ ] changed files enumerated with type (modified/added/deleted)
- [ ] risk classified

### FORBIDDEN
- Fetching `patches:{mode:"all"}` before seeing `changedFiles` list
- Repeating existing review comments
- Using shell commands for code reading in Local Mode (MUST use `localGetFileContent`)
- Guessing `lineHint` without `localSearchCode` first

</context_gate>

---

## Phase 3: Checkpoint (Mandatory)

<checkpoint_gate>
**STOP. Present summary. Ask for direction.**

**PR Mode template:**
```
PR REVIEW: [prNumber] — [title]
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Overview: [what this PR does]
Author: [author] | State: [open/merged] | Checks: [pass/fail]
Files Changed: [N] files | +[add] / -[del] lines
Risk: [HIGH/MEDIUM/LOW] — [reason]
Mode: [Quick/Full]

Areas:
  • [Area 1]: [files]
  • [Area 2]: [files]

Existing Review State: [approved by X / changes requested by Y / N comments]
Guidelines Loaded: [N sources] OR "None"

Concerns spotted:
  • [concern if any]
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**Local Mode template:**
```
LOCAL REVIEW: [branch]
━━━━━━━━━━━━━━━━━━━━━━

Staged: [N] files | Unstaged: [N] files | Untracked: [N]
Risk: [HIGH/MEDIUM/LOW] | Mode: [Quick/Full]
Areas: [list]
━━━━━━━━━━━━━━━━━━━━━━
```

Ask:
1. "Which areas to focus on?"  
2. "Full review or specific concerns?"

**STOP. Wait for response.** If "just give me the summary" → skip to Phase 6.
</checkpoint_gate>

---

## Phase 4: Analysis

<analysis_gate>

**Execute per user direction from Phase 3. Respect guidelines from Phase 1.**

### Search Query Plan (list 3-5, then execute)
```
Query 1: [tool] — [pattern] — [goal]
Query 2: [tool] — [pattern] — [goal]
...
```

### Required Checks (ALL phases, ALL modes)

**[A] Guidelines Compliance** (if loaded):
- Check each changed file against every loaded guideline rule
- Flag violations: `[GUIDELINE: {source} — {rule}]`

**[B] Flow Impact** (for any function/method/type change):
- Apply Flow Analysis Protocol (see table above)
- Document blast radius: symbol → callers → breaking change (yes/no)

**[C] Domain Analysis** (per user focus):
- **Bug**: crashes, data corruption, null access, race conditions, API misuse
- **Security**: injection, XSS, hardcoded secrets, auth bypass, data exposure, GDPR/HIPAA
- **Architecture**: coupling, circular deps, pattern violations, wrong module placement
- **Performance**: O(n²), blocking ops, unbatched calls, memory leaks, missing cache
- **Error Handling**: swallowed exceptions, missing log context, unclear error messages
- **Code Quality**: naming violations, TODO/FIXME in new code, magic numbers, DRY violations
- **Flow Impact**: callers/consumers of changed symbols

**[D] PR-Specific Checks:**
- Patch pagination: if `contentPagination.patches.hasMore` → fetch next page before concluding
- Comment pagination: if `contentPagination.commentBody.hasMore` → fetch more
- Verify ALL existing review comments — check if they were fixed; re-flag if not

**[E] Commit Context** (when suspicious changes):
```
ghHistoryResearch(type:"commits", path="changed-file.ts", includeDiff:true)
→ Find original introduction of problematic pattern
→ Extract PR from messageHeadline → fetch that PR for original context
```

### Gate Check
- [ ] All search queries executed with evidence
- [ ] Guidelines compliance checked
- [ ] Flow impact traced for all modified functions (LSP in Local Mode, `ghSearchCode` in remote-only)
- [ ] All user focus areas covered
- [ ] Findings compiled with confidence levels

### FORBIDDEN
- Areas user explicitly excluded
- Analyzing unchanged code (only '+' diff lines)
- Using `github*` tools for code reading in Local Mode
- Guessing `lineHint` without `localSearchCode`
- Fabricating results when tool returns empty — broaden query or document the gap

</analysis_gate>

---

## Phase 5: Finalize

1. **Dedupe** — merge findings with same root cause; drop any already in existing PR comments
2. **Verify confidence** — MED confidence → research more or mark uncertain; wrong → delete
3. **Guidelines cross-check** — every finding verified against loaded guidelines; guideline wins on conflict
4. **Each finding MUST have**: HIGH/MED confidence + `file:line` + diff fix
5. **Cap at 5-7 key issues** — prioritize: Security > Bug > Flow > Arch > Perf > Quality

---

## Phase 6: Report

**Step 1: Chat summary (always first):**

**PR Mode:**
```
REVIEW COMPLETE: [prNumber]
━━━━━━━━━━━━━━━━━━━━━━━━━━━

Recommendation: APPROVE / REQUEST_CHANGES / COMMENT
Risk: [HIGH/MEDIUM/LOW]

High Priority ([N]):
  1. [title] — [path]:[line]

Medium Priority ([N]):
  1. [title] — [path]:[line]

Low Priority ([N]):
  1. [title]

Guidelines: [N violations / All pass / None loaded]
Existing Comments: [N resolved / N still open]
━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**Local Mode:**
```
REVIEW COMPLETE: Local ([branch])
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Scope: [staged/unstaged/both] — [N] files, [N] lines
Recommendation: LOOKS_GOOD / NEEDS_CHANGES
Risk: [HIGH/MEDIUM/LOW]
[same issue structure]
Next: [Run tests / Fix issues / Split commits / Ready to commit]
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**History Mode:**
```
HISTORY: [file or repo]
━━━━━━━━━━━━━━━━━━━━━━━

[date] [sha] [author] — [message] (PR: #N)
[date] [sha] [author] — [message]
...

Key findings:
  • [file] last modified by [author] on [date] in PR [N]
  • [pattern] introduced in commit [sha] — original context: [PR title]
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**Step 2: Ask before writing doc:**  
"Would you like me to create the detailed review document?"  
- Yes → write to `.octocode/reviewPR/{session}/PR_{N}.md` (PR) or `.octocode/reviewLocal/{session}/REVIEW_{branch}.md` (Local)
- No → continue discussion

**FORBIDDEN:** Writing files without explicit user approval. Using `#N` in finding labels.

---

## Multi-Agent Parallelization

| Files | Agents | Who |
|-------|--------|-----|
| ≤5 | 0 (single-pass) | — |
| 6-15 | 2 | A (Flow) + C (Arch+Quality) |
| 16-30 | 3 | A (Flow) + B (Security) + C (Arch+Quality) |
| 30+ | 4 | A + B + C + D (Guidelines, only if loaded) |

**Spawn ALL agents in a SINGLE message. Wait for ALL to return before Phase 5.**

**Agent A — Flow Impact:** `localSearchCode` → `lspGetSemantics(type="callers")` → `lspGetSemantics(type="references")` → document blast radius per symbol

**Agent B — Security + Errors:** Scan `+` lines for injection/XSS/secrets/auth-bypass/swallowed exceptions → `localSearchCode` for patterns → `ghGetFileContent(matchString)` for context

**Agent C — Arch + Quality + Perf:** `ghViewRepoStructure` / `localViewStructure` → check pattern alignment, coupling, O(n²), naming, TODO in new code

**Agent D — Guidelines + Dupes (only if guidelines loaded):** Cross-check every changed file against guidelines context → `localSearchCode`/`ghSearchCode` for reusable utilities already in repo

**Quick mode: FORBIDDEN to spawn agents.**

---

## Verification Checklist

- [ ] Target/mode resolved (PR / Local / History)
- [ ] Tool API used correctly (`ghHistoryResearch` with right content selectors)
- [ ] `localSearchCode` called before any `lspGetSemantics` (never guess `lineHint`)
- [ ] Existing PR comments noted and deduped
- [ ] Phase 4 analysis complete with evidence
- [ ] All findings: `file:line` + confidence + diff fix
- [ ] No `#N` notation in finding labels or references
- [ ] User approved before writing any file

---

## References

- [Execution Lifecycle](references/execution-lifecycle.md)
- [Flow Analysis Protocol](references/flow-analysis-protocol.md)
- [Domain Reviewers](references/domain-reviewers.md)
- [Parallel Agent Protocol](references/parallel-agent-protocol.md)
- [Output Template](references/output-template.md)
- [Review Guidelines](references/review-guidelines.md)
- [Verification Checklist](references/verification-checklist.md)
