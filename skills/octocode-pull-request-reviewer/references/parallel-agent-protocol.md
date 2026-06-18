# Multi-Agent Parallelization & Swarm Strategy

## When to Parallelize

| PR Size | Files | Mode | Agent Strategy |
|---------|-------|------|----------------|
| Small | ≤5 | Quick | No agents — single-pass review |
| Medium | 6-15 | Full | 2 parallel agents (Flow + Domains) |
| Large | 16-30 | Full | 3 parallel agents (Flow + Security + Domains) |
| XL | 30+ | Full | 4 parallel agents (Flow + Security + Architecture + Domains) |

**IF** Quick mode → FORBIDDEN to spawn agents. Single-pass only.
**IF** Full mode AND >5 files → MUST use parallel agents for Phase 4 (Analysis).

---

## Swarm Architecture

```
                    ┌─────────────────────┐
                    │   ORCHESTRATOR (you) │
                    │   Phases 1-3, 5-6    │
                    └──────────┬──────────┘
                               │ Phase 4: Spawn agents
                    ┌──────────┼──────────┐──────────┐
                    ▼          ▼          ▼          ▼
              ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐
              │ Agent A   │ │ Agent B   │ │ Agent C   │ │ Agent D   │
              │ Flow      │ │ Security  │ │ Arch +    │ │ Guidelines│
              │ Impact    │ │ + Errors  │ │ Quality   │ │ + Dupes   │
              └─────┬─────┘ └─────┬─────┘ └─────┬─────┘ └─────┬─────┘
                    │             │             │             │
                    └──────────┬──┴─────────────┴─────────────┘
                               ▼
                    ┌─────────────────────┐
                    │   ORCHESTRATOR      │
                    │   Merge + Dedupe    │
                    │   Phase 5-6         │
                    └─────────────────────┘
```

**CRITICAL: All agent Task calls MUST be in a SINGLE message for true parallel execution.**

---

## Agent Definitions

### Agent A: Flow Impact Analyst
- **Scope**: Flow Impact domain + blast radius mapping
- **Tools**: `localSearchCode` → `lspGetSemantics(type="callers")` → `lspGetSemantics(type="references")` → `ghSearchCode`
- **Task**: For every modified function/method/type in the diff:
  1. Call `localSearchCode` to get lineHint for each symbol
  2. Call `lspGetSemantics(type="callers", symbolName, lineHint, format:"compact")` to find all callers
  3. Call `lspGetSemantics(type="references", symbolName, lineHint, groupByFile:true)` for changed types/interfaces
  4. Document: symbol name, file:line, caller count, breaking change (yes/no)
- **Output**: List of `{ symbol, file:line, callers: [{file:line, impact}], breaking: bool }`
- **Prompt template**:
  ```
  You are a Flow Impact Analyst. Review the following PR diff and trace ALL
  modified functions/methods/types to find their callers and consumers.

  PR diff: {diff_summary}
  Modified symbols: {list_of_changed_functions_types}
  Repo: {owner}/{repo}

  For EACH modified symbol:
  1. Use localSearchCode(pattern="symbolName") to get lineHint
  2. Use lspGetSemantics(type="callers", symbolName, lineHint, format:"compact") for functions
  3. Use lspGetSemantics(type="references", symbolName, lineHint, groupByFile:true) for types/interfaces
  4. Document the blast radius

  Return findings as structured list with file:line citations.
  FORBIDDEN: Guessing lineHint. ALWAYS search first.
  ```

### Agent B: Security & Error Handling Reviewer
- **Scope**: Security scan + Error Handling domain
- **Tools**: `localSearchCode` → `ghGetFileContent(matchString=...)` → `localGetFileContent`
- **Task**:
  1. Scan changed files for: hardcoded secrets, SQL injection, XSS, data exposure, auth bypass
  2. Check error handling: swallowed exceptions, missing context, unclear messages
  3. Verify input validation on new endpoints/functions
  4. Check for regulatory compliance patterns (GDPR, HIPAA)
- **Output**: List of `{ issue, file:line, severity, confidence, fix }`
- **Prompt template**:
  ```
  You are a Security & Error Handling Reviewer. Scan the following PR diff
  for security vulnerabilities and error handling issues.

  PR diff: {diff_content}
  Changed files: {file_list}

  Security checks: injection, XSS, data exposure, auth bypass, hardcoded secrets
  Error handling checks: swallowed exceptions, missing context, unclear messages

  Use localSearchCode to find patterns, ghGetFileContent for context.
  Return findings with file:line, severity, confidence, and fix.
  ONLY flag issues in CHANGED code ('+' lines).
  ```

### Agent C: Architecture & Code Quality Reviewer
- **Scope**: Architecture domain + Code Quality domain + Performance domain
- **Tools**: `ghViewRepoStructure` → `localViewStructure` → `localSearchCode` → `ghGetFileContent`
- **Task**:
  1. Check changed code against repo patterns and conventions
  2. Detect: coupling, circular deps, wrong module placement, naming violations
  3. Performance: O(n²), blocking ops, missing cache, unbatched operations
  4. Check for TODO/FIXME in new code
- **Output**: List of `{ issue, domain, file:line, severity, confidence, fix }`
- **Prompt template**:
  ```
  You are an Architecture & Code Quality Reviewer. Analyze the following PR diff
  for architectural issues, code quality problems, and performance concerns.

  PR diff: {diff_content}
  Changed files: {file_list}
  Repo structure: {structure_summary}

  Check: pattern violations, coupling, naming, O(n²), blocking ops, magic numbers
  Use ghViewRepoStructure to understand repo layout.
  Use localSearchCode to find existing patterns for comparison.
  Return findings with file:line, domain, severity, confidence, and fix.
  ONLY flag issues in CHANGED code ('+' lines).
  ```

### Agent D: Guidelines & Duplicate Code Reviewer (only if guidelines loaded)
- **Scope**: Guidelines compliance + Duplicate Code domain
- **Tools**: `localSearchCode` → `ghSearchCode` → `localGetFileContent` → `ghGetFileContent`
- **Task**:
  1. Check each changed file against loaded guidelines (from Phase 1)
  2. Search for existing utilities/patterns that new code could reuse
  3. Flag DRY violations across the codebase
- **Output**: List of `{ guideline_source, rule, status: PASS/VIOLATION, file:line }` + duplicate findings
- **Prompt template**:
  ```
  You are a Guidelines & Duplicate Code Reviewer.

  Guidelines context:
  {guidelines_context_from_phase_1}

  PR diff: {diff_content}
  Changed files: {file_list}

  Task 1: For each changed file, check compliance against every loaded guideline rule.
  Task 2: Use localSearchCode/ghSearchCode to find existing utilities that new code duplicates.
  Return: guidelines compliance table + duplicate code findings with file:line.
  ```

---

## Scaling Rules

| Agents | Condition | Which Agents |
|--------|-----------|-------------|
| 0 | Quick mode OR ≤5 files | None — single-pass |
| 2 | 6-15 files, no guidelines | A (Flow) + C (Arch+Quality) |
| 3 | 16-30 files OR guidelines loaded | A (Flow) + B (Security) + C (Arch+Quality) |
| 3 | 6-15 files + guidelines loaded | A (Flow) + C (Arch+Quality) + D (Guidelines) |
| 4 | 30+ files + guidelines loaded | A + B + C + D (all agents) |

---

## Merge Protocol (Phase 5 — Orchestrator)

After all agents return, the orchestrator MUST:

1. **Collect**: Gather all findings from all agents into a single list
2. **Dedupe**: Remove findings with the same root cause or same file:line
   - **IF** two agents report the same issue → keep the one with higher confidence
   - **IF** same file:line but different domains → merge into single finding, list both domains
3. **Cross-check**: Verify agent findings against existing PR comments (Phase 2)
4. **Prioritize**: Sort by severity (HIGH → MED → LOW), then by domain weight:
   - Security > Bug > Flow Impact > Architecture > Performance > Quality > Duplicates
5. **Cap**: Select top ~5-7 most impactful findings
6. **Enrich**: For each finding, ensure file:line + confidence + code fix exists

**FORBIDDEN:**
- Spawning agents in Quick mode
- Spawning >4 agents (diminishing returns, context overhead)
- Agents modifying files or writing output directly
- Spawning agents sequentially (MUST be single-message parallel)
- Proceeding to Phase 6 before ALL agents have returned
