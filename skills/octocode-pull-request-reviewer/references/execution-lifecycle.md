# Execution Lifecycle

<execution_lifecycle>

### Phase 1: Guidelines & Context Gateway (MANDATORY)

<guidelines_gate>
**STOP. Before fetching changes, ask the user for review guidelines and context.**

### Pre-Conditions
- [ ] Pre-Flight dependency check passed
- [ ] Review target identified (PR number/URL for PR Mode, or local changes confirmed for Local Mode)

### Actions (REQUIRED)

**Step 1: Check for existing context files.**
- **IF** Local Mode OR workspace IS the PR repo → Call `localFindFiles` to check for:
  - `.octocode/pr-guidelines.md`
  - `.octocode/context/context.md`
  - `CONTRIBUTING.md`
  - `AGENTS.md`
- **IF** PR Mode AND workspace is NOT the PR repo → Call `ghSearchCode` with `match="path"` and `keywordsToSearch=["pr-guidelines", "CONTRIBUTING", "AGENTS"]` scoped to the PR's `owner/repo`
- **IF** any files found → Read them using the appropriate tool (`localGetFileContent` or `ghGetFileContent`) and inform user: "I found the following context files: [list]. I'll use these as review guidelines."

**Step 2: Ask user (MANDATORY).**
Ask user:
> "Do you have any **guidelines files** or **context documents** I should use for this review?"
>
> You can provide:
> - A file path (e.g., `docs/review-guidelines.md`)
> - Inline text with rules/context
> - Or say **"skip"** to proceed without additional guidelines

**STOP. Wait for user response.**

**Step 3: Process user-provided guidelines.**
- **IF** user provides file path(s) → Read each file using `localGetFileContent` (local repo) or `ghGetFileContent` (remote repo)
- **IF** user provides inline text → Store as review context
- **IF** user says "skip" or "no" → Proceed with default review domains only
- **IF** existing context files were found (Step 1) AND user says "skip" → Still use the auto-discovered files

**Step 4: Build guidelines context.**
Combine all sources into a structured **guidelines context**:

```
GUIDELINES CONTEXT:
─────────────────────
Source: [file path or "user-provided"]
Priority: [1-Highest / 2-High / 3-Medium / 4-Baseline]
Rules:
  - [Rule 1]: [description]
  - [Rule 2]: [description]
─────────────────────
(repeat for each source)
```

| Source | Priority | Usage |
|--------|----------|-------|
| User-provided guidelines | 1 — Highest | Override default rules where specified |
| `.octocode/pr-guidelines.md` | 2 — High | Project-specific review rules |
| `.octocode/context/context.md`, `CONTRIBUTING.md`, `AGENTS.md` | 3 — Medium | Coding standards & conventions |
| Default domain reviewers | 4 — Baseline | Used when no guidelines override |

The guidelines context MUST be referenced in Phase 4 (Analysis), Phase 5 (Finalize), and Phase 6 (Report).

### Gate Check
- [ ] User was asked for guidelines
- [ ] All discovered files read and parsed
- [ ] Guidelines context built (or confirmed empty)

### FORBIDDEN
- Proceeding to Phase 2 without asking the user for guidelines
- Ignoring user-provided guidelines during later phases
- Treating guidelines as optional once provided — they are REQUIRED review criteria

### ALLOWED
- Reading files via Octocode MCP tools
- Asking user clarifying questions about guidelines

### On Failure
- **IF** file path provided but file not found → **THEN** inform user, ask for correct path
- **IF** file unreadable → **THEN** inform user, proceed with remaining sources
</guidelines_gate>

---

### Phase 2: Context

<context_gate>

### Pre-Conditions
- [ ] Phase 1 (Guidelines) completed
- [ ] Guidelines context built (or confirmed empty)

### Actions — PR Mode (REQUIRED — all via `ghHistoryResearch`)
1. **Orientation (always first — cheapest)**: `ghHistoryResearch(type:"prs", prNumber:N, content:{metadata:true, changedFiles:true, reviews:true})`
   → title, author, file list, additions/deletions, review state, CI checks
2. **Existing comments (dedup guard)**: `ghHistoryResearch(type:"prs", prNumber:N, content:{comments:{discussion:true, reviewInline:true, includeBots:false}})`
   - MUST note ALL existing comments — never repeat them in findings
   - If `contentPagination.commentBody.hasMore` → fetch next page via `commentBodyOffset` from hints[]
3. **Targeted diffs (high-risk files first)**: `ghHistoryResearch(type:"prs", prNumber:N, content:{patches:{mode:"selected", files:[HIGH_RISK_FILES]}})`
   - Use `mode:"all"` only when total files ≤15; `mode:"selected"` otherwise
   - If `contentPagination.patches.hasMore` → advance `charOffset` from hints[] — do NOT compute manually
4. **Classify risk**: HIGH (Auth/API/Data/Logic) vs LOW (Docs/CSS/Config)
5. **PR Health Check**:
   - Flag large PRs (>500 lines) → suggest splitting
   - Missing description → flag
   - Can PR be split into independent sub-PRs?
6. **Group changed files by functional area**: List each area with its files
7. **Commit progression (optional)**: `ghHistoryResearch(type:"prs", prNumber:N, content:{commits:{list:true}})`
   → Understand development arc; extract `#N` refs from `messageHeadline` → re-call with that `prNumber` for cross-reference
8. **Check for ticket/issue reference** → verify requirements alignment
9. **Select review mode**: Apply Review Mode Selector from Global Rules (Quick or Full)

**One-shot full fetch (≤30 files):** `ghHistoryResearch(type:"prs", prNumber:N, reviewMode:"full")` → all surfaces in one call

### Actions — Local Mode (REQUIRED — Octocode local tools + shell git)

1. **Identify changed files**: Run `git status` to list staged, unstaged, and untracked files
2. **Collect diffs**:
   - Staged changes: `git diff --staged`
   - Unstaged changes: `git diff`
   - Combined view: `git diff HEAD` (if both staged + unstaged exist)
   - **IF** user specifies "staged only" or "unstaged only" → respect that scope
3. **Get branch context**: Run `git branch --show-current` and `git log --oneline -10` for recent commit history
4. **Read changed file context** (for each changed file):
   - Call `localGetFileContent` with `matchString` targeting the changed functions/areas
   - Call `localViewStructure` on parent directories to understand module placement
5. **Classify risk**: Same criteria as PR Mode — HIGH (Logic/Auth/API/Data) vs LOW (Docs/CSS/Config)
6. **Group changed files by functional area**: Same as PR Mode
7. **Changes Health Check**:
   - Flag large change sets (>500 lines) → suggest splitting into smaller commits
   - Identify if changes span unrelated areas → suggest separate commits
8. **Select review mode**: Apply Review Mode Selector from Global Rules (Quick or Full)

### Actions — Local Mode (File Scope) (when user requests a specific file path)

> Applies when the user provides a specific file path (e.g., "review src/auth/login.ts"). Scoped analysis — do NOT expand to full-repo review.

1. **Verify file exists**: Call `localFindFiles` or `localViewStructure` to confirm the path
   - **IF** file not found → STOP, ask user for the correct path
2. **Read the target file**: Call `localGetFileContent` on the requested file
3. **Map immediate dependencies**:
   - Call `localSearchCode` on the file to identify imports and exports
   - Call `lspGetSemantics(type="references", groupByFile:true)` on exported symbols to find direct consumers (1 hop only)
   - Call `lspGetSemantics(type="callers")` on public functions to find direct callers
4. **Classify risk**: Based on the file's role (auth/data/config = HIGH, utils/docs = LOW)
5. **Select review mode**: Typically Quick unless the file is high-risk or complex

### Gate Check — PR Mode
- [ ] PR metadata fetched
- [ ] PR diff fetched
- [ ] Existing comments fetched and noted
- [ ] Risk classified
- [ ] Changed files grouped by functional area
- [ ] Review mode selected (Quick / Full)

### Gate Check — Local Mode
- [ ] `git status` output collected
- [ ] Diffs collected (staged and/or unstaged as applicable)
- [ ] Changed files enumerated with change type (modified/added/deleted)
- [ ] Risk classified
- [ ] Changed files grouped by functional area
- [ ] Review mode selected (Quick / Full)

### Gate Check — Local Mode (File Scope)
- [ ] Target file verified to exist
- [ ] File content read via `localGetFileContent`
- [ ] Immediate dependencies mapped (imports, exports, callers)
- [ ] Risk classified
- [ ] Review mode selected (Quick / Full)

### FORBIDDEN
- **PR Mode**: Proceeding without fetching existing comments first; skipping PR health check
- **Local Mode**: Using `cat` / `head` / shell to read file content (MUST use `localGetFileContent`)
- **Both**: Skipping risk classification

### ALLOWED
- **PR Mode**: Octocode MCP `github*` tool calls
- **Local Mode**: Octocode MCP `local*` tools + shell `git` commands (status, diff, log, branch)
- **Both**: Task/todo tracking tool for progress tracking

### On Failure
- **PR Mode**: **IF** PR not found → **THEN** ask user for correct PR number/URL
- **PR Mode**: **IF** diff too large (>2000 lines) → **THEN** use `patches:{mode:"selected", files:[HIGH_RISK]}`, focus on high-risk files first; paginate via `charOffset` from hints[]
- **Local Mode**: **IF** no changes detected → **THEN** inform user, suggest checking the correct branch
- **Local Mode**: **IF** diff too large → **THEN** ask user to scope (e.g., "staged only" or specific files)
</context_gate>

---

### Phase 3: User Checkpoint (MANDATORY)

<checkpoint_gate>
**STOP. Present findings and ask user for direction before deep analysis.**

### Pre-Conditions
- [ ] Phase 2 (Context) completed
- [ ] Changes collected: PR metadata + diff + comments (PR Mode) OR git diff + status (Local Mode)
- [ ] Risk classified and files grouped

### Actions (REQUIRED)

**Step 1: Present TL;DR Summary using the appropriate template:**

**PR Mode template:**
```
PR REVIEW: #{prNumber} — {title}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Overview: {1-2 sentence description of what this PR does}

Files Changed: {count} files in {N} areas:
  • {Area 1}: {file1}, {file2}
  • {Area 2}: {file3}
  ...

Risk Assessment: {HIGH / MEDIUM / LOW} — {reasoning}

Review Mode: {Quick / Full} — {reasoning}

Key Areas:
  1. {Area name} — {why it matters}
  2. {Area name} — {why it matters}
  ...

Guidelines Loaded: {count} sources ({list names}) OR "None"

Potential Concerns:
  • {concern 1, if any}
  • {concern 2, if any}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**Local Mode template:**
```
LOCAL CHANGES REVIEW: {branch} — {scope description}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Overview: {1-2 sentence description of what these changes do}

Change Scope:
  • Staged: {count} files ({total lines})
  • Unstaged: {count} files ({total lines})
  • Untracked: {count} files

Files Changed: {count} files in {N} areas:
  • {Area 1}: {file1}, {file2}
  • {Area 2}: {file3}
  ...

Risk Assessment: {HIGH / MEDIUM / LOW} — {reasoning}

Review Mode: {Quick / Full} — {reasoning}

Key Areas:
  1. {Area name} — {why it matters}
  2. {Area name} — {why it matters}
  ...

Guidelines Loaded: {count} sources ({list names}) OR "None"

Potential Concerns:
  • {concern 1, if any}
  • {concern 2, if any}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**Step 2: Ask user (MANDATORY).**
1. "Which areas would you like me to focus on?" (list identified areas as options)
2. "Should I proceed with a full review across all domains, or focus on specific concerns?"

**STOP. Wait for user response before proceeding to Phase 4.**

**Step 3: Process user response.**
- **IF** user specifies focus areas → Store as review focus, apply in Phase 4
- **IF** user provides additional context → Append to guidelines context
- **IF** user says "proceed with full review" → Continue to Phase 4 with all domains
- **IF** user says "just give me the summary" → Jump to Phase 6 with current findings

### Gate Check
- [ ] TL;DR Summary presented to user
- [ ] User asked for focus direction
- [ ] User response received and stored

### FORBIDDEN
- Proceeding to Phase 4 without user response
- Ignoring user-specified focus areas

### ALLOWED
- Presenting summary in chat
- Asking clarifying questions

### On Failure
- **IF** user unresponsive → **THEN** wait (do NOT proceed without direction)
</checkpoint_gate>

---

### Phase 5: Finalize

<finalize_gate>

### Pre-Conditions
- [ ] Phase 4 (Analysis) completed
- [ ] Findings list compiled with confidence levels

### Actions (REQUIRED)
1. **Dedupe**: Cross-check findings against existing PR comments from Phase 2. MUST merge findings with the same root cause.
2. **Refine**: For each finding with MED or lower confidence → research more via Octocode MCP or mark as uncertain
   - **UNCHANGED**: Suggestion verified correct
   - **UPDATED**: New context improves suggestion
   - **INCORRECT**: Context proves suggestion wrong → MUST delete
3. **Verify against guidelines** (REQUIRED if guidelines were loaded in Phase 1):
   - Cross-check each finding against the guidelines context
   - MUST flag guideline violations explicitly with format: `[GUIDELINE: {source} — {rule}]`
   - Confirm no guideline-required checks were missed
   - **IF** a finding contradicts a guideline → guideline wins (document the conflict per Global Rules precedence table)
4. **Verify each finding has**:
   - HIGH or MED confidence level
   - Exact file:line location
   - Actionable code fix (diff format)
   - **PR Mode — Previous Comments Resolution**: MUST verify that comments from previous reviews were fixed. If not, re-flag as unresolved.
   - **Local Mode**: No previous comments to check (skip this sub-step)
5. **Limit to most impactful findings** (max ~5-7 key issues). Prioritize by: HIGH priority first, then by domain severity.

### Gate Check
- [ ] No duplicate findings (vs existing PR comments)
- [ ] All findings have HIGH/MED confidence
- [ ] All findings have file:line + code fix
- [ ] Guidelines compliance verified (if applicable)
- [ ] Previous review comments checked for resolution
- [ ] ≤7 key issues selected

### FORBIDDEN
- Including LOW confidence findings without explicit uncertainty marker
- Including findings already raised in existing PR comments
- Omitting code fix for any finding

### ALLOWED
- Additional Octocode MCP research to verify uncertain findings
- Asking user for clarification on ambiguous cases

### On Failure
- **IF** too many findings (>10) → **THEN** prioritize by severity, move LOW to "Additional Notes"
- **IF** finding lacks evidence → **THEN** delete or mark as LOW confidence with caveat
</finalize_gate>

---

### Phase 6: Report

<report_gate>

### Pre-Conditions
- [ ] Phase 5 (Finalize) completed
- [ ] Findings list finalized (≤7 key issues)
- [ ] All findings verified with confidence + fix

### Actions (REQUIRED)

**Step 1: Chat Summary (MANDATORY).**
Present in chat before creating any document:

**PR Mode:**
```
REVIEW COMPLETE: #{prNumber}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Recommendation: {APPROVE / REQUEST_CHANGES / COMMENT}
Risk Level: {HIGH / MEDIUM / LOW}

High Priority ({count}):
  1. {title} — {path}:{line}
  ...

Medium Priority ({count}):
  1. {title} — {path}:{line}
  ...

Low Priority ({count}):
  1. {title}
  ...

Guidelines: {X violations / All pass / No guidelines loaded}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**Local Mode:**
```
REVIEW COMPLETE: Local Changes ({branch})
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Scope: {staged/unstaged/both} — {file count} files, {line count} lines
Recommendation: {LOOKS_GOOD / NEEDS_CHANGES / COMMENT}
Risk Level: {HIGH / MEDIUM / LOW}

High Priority ({count}):
  1. {title} — {path}:{line}
  ...

Medium Priority ({count}):
  1. {title} — {path}:{line}
  ...

Low Priority ({count}):
  1. {title}
  ...

Guidelines: {X violations / All pass / No guidelines loaded}

Suggested Next Steps:
  • {Run tests / Fix issues / Split into commits / Ready to commit}
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

**Step 2: Ask before creating doc (MANDATORY).**
Ask user: "Would you like me to create the detailed review document?"
- **IF** yes → Generate per output structure below
- **IF** no → Continue discussion or provide additional analysis

**Step 3: Generate document (after user approval only).**
- MUST ensure all findings have: location, confidence, concise problem, code fix
- MUST number issues sequentially across all priorities
- **PR Mode**: Write to `.octocode/reviewPR/{session-name}/PR_{prNumber}.md`
- **Local Mode**: Write to `.octocode/reviewLocal/{session-name}/REVIEW_{branch}_{timestamp}.md`

### Gate Check
- [ ] Chat summary presented
- [ ] User asked before creating document
- [ ] User approved document creation (if generating)

### FORBIDDEN
- Writing `.octocode/reviewPR/...` without explicit user approval
- Omitting chat summary
- Generating document without asking first

### ALLOWED
- Chat output (summary)
- File write (ONLY after user approval)

### On Failure
- **IF** user declines document → **THEN** continue discussion, offer alternative analysis
- **IF** write fails → **THEN** output document content in chat instead
</report_gate>

</execution_lifecycle>
