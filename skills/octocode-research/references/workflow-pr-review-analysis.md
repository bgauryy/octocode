# PR Review Analysis

Use after `workflow-pr-review.md` collects target, guidelines, context, and checkpoint scope.

## Domain Reviewers

| Domain | Detect | HIGH priority | MED priority | Skip |
|---|---|---|---|---|
| Bug | Runtime errors, logic flaws, data corruption, resource leaks, race conditions, type violations, API misuse | Crashes, data corruption, security breach, null access in a hot path | Edge-case errors, uncertain race conditions | Try/catch without cleanup need, compiler-caught issues |
| Architecture | Pattern violations, tight coupling, circular deps, mixed concerns, leaky abstractions | Breaking public API, circular deps causing bugs | Significant pattern deviations, tech-debt increase | Single-file organization, framework-standard patterns |
| Performance | O(n^2) where O(n) is possible, blocking ops, missing cache, unbatched ops, memory leaks | O(n^2) on large datasets, memory leaks, blocking the main thread | Moderate inefficiency in frequent paths | Negligible impact, theoretical improvements |
| Code Quality | Naming violations, convention breaks, visible typos, magic numbers, TODO in new code | Typos in a public API/endpoint | Internal naming issues, DRY violations, convention deviations | Personal style, linter-handled formatting |
| Duplicate Code | Missed opportunities to reuse existing code/utilities/patterns | Missing use of a critical utility that could prevent bugs | Duplication violating DRY across files | Intentional duplication for clarity |
| Error Handling | Poor error messages, unclear logs, swallowed exceptions, missing debug context | Swallowed exceptions hiding critical failures | Unclear error messages, missing log context | Internal service calls in trusted environments |
| Flow Impact | How changes alter execution flow, data paths, system behavior — trace with the Flow Analysis Recipes above | Changes that break callers, alter critical paths, change data-flow semantics | Flow changes requiring dependent-code updates, altered return values/types | Internal refactors with unchanged external behavior |

Global exclusions — never suggest: compiler/linter errors (tooling already catches these), unchanged code (no `+` prefix), test implementation details unless broken, generated/vendor files, speculative "what if" scenarios, issues already raised in existing PR comments.

## Review Confidence Model

Keep two axes distinct — do not conflate them:

- **Severity** (impact if true): `HIGH`/`MED`/`LOW`, per the domain table above.
- **Confidence** (how sure the evidence makes you): `confirmed`/`likely`/`uncertain`, per `algorithm.md`'s evidence grades — same vocabulary as every other workflow in this skill.

A HIGH-confidence typo is still LOW severity.
A `likely`-confidence security flaw still gets flagged, but mark it likely rather than confirmed.
Include a finding when it touches new/changed code, is `confirmed` or `likely`, and is actionable.
Skip findings that need more evidence, or mark them `uncertain` instead of asserting them.

Mindset: focus on changed code only.
Cover added lines, modified implementations, and deleted code only when the removal creates a new risk.
Think like a parser: trace imports to definitions, then follow entry -> propagation -> termination.
Use `localSearchCode` or AST structural matches to confirm the shape before calling it a finding.

## Analysis

Run for every user-specified focus area (or all domains in Full mode without a stated focus):

1. List 3-5 search queries aligned with the focus, execute each, and name the goal per query.
2. Guidelines compliance: check each changed file against the loaded guidelines context; flag violations with a specific reference: `[GUIDELINE: <source> — <rule>]`.
3. Flow impact analysis (required for every function/method change): apply the matching Flow Analysis Recipe; document blast radius.
4. Validate schemas/APIs/dependencies with `matchString`-targeted reads (`ghGetFileContent`/`localGetFileContent` + `localSearchCode`).
5. Assess impact per angle, prioritizing the user's stated focus: architectural (structure, pattern alignment), integration (affected systems/patterns), risk (race conditions, performance, security), business (UX, metrics, operational cost), cascade (could this cause other problems downstream).
6. Identify edge cases in the changed logic.
7. Security scan: injection, XSS, data exposure, auth bypass, hardcoded secrets, regulatory-compliance patterns where relevant.
8. Scan new code (`+` lines only) for TODO/FIXME.
9. For high-risk changes, assess whether a rollback strategy or feature flag is needed.
10. Local Mode only: if changes are substantial, suggest running the project's test/lint suite before finalizing.

Do not analyze areas the user explicitly excluded at Checkpoint.
Do not use `gh*` tools for code reading in Local Mode.
In PR Mode outside the PR repo, use GitHub tools for code reads.
If a search returns nothing, broaden the query or change tools before concluding absence.
If flow tracing dead-ends, document the limitation and proceed with available evidence.

## Multi-Agent Parallelization (only if your runtime supports spawning subagents)

Applies to both PR and Local Mode; skip entirely in Quick mode or file-scope review (single-pass only).

| Files changed | Guidelines loaded? | Agents |
|---|---|---|
| ≤5 (Quick) | either | none — single-pass |
| 6-15 | no | 2: Flow Impact + Architecture/Quality |
| 6-15 | yes | 3: Flow Impact + Architecture/Quality + Guidelines & Duplicates |
| 16-30 | either | 3: Flow Impact + Security & Error Handling + Architecture/Quality |
| 30+ | no | 3: Flow Impact + Security & Error Handling + Architecture/Quality |
| 30+ | yes | 4: all — Flow Impact + Security & Error Handling + Architecture/Quality + Guidelines & Duplicates |

Spawn all agents for a batch in a single message.
Each agent uses the same mode-appropriate tools as above.
Local Mode agents do code reading with local tools.

- **Flow Impact**: for every modified symbol, `localSearchCode` -> `lspGetSemantics(type: "callers"/"references")` (or `ghSearchCode`/`ghGetFileContent` remotely) -> document `{symbol, file:line, callers, breaking: bool}`.
- **Security & Error Handling**: scan changed files for injection/XSS/data exposure/auth bypass/hardcoded secrets and swallowed exceptions/missing error context; only flag `+` lines.
- **Architecture & Code Quality**: compare changed code against existing repo patterns (`ghViewRepoStructure`/`localViewStructure` for layout, `localSearchCode` for existing patterns); flag coupling, naming, performance smells, TODO/FIXME in new code.
- **Guidelines & Duplicates** (only if guidelines loaded): check each changed file against every loaded rule; search for existing utilities the new code should have reused instead of duplicating.

Merge (orchestrator): collect all findings and dedupe by root cause or same `file:line`.
Keep the higher-confidence finding and merge cross-domain hits into one finding listing both domains.
Cross-check existing PR comments before adding new findings.
Prioritize Security > Bug > Flow Impact > Architecture > Performance > Quality > Duplicates.
Cap to the top 5-7 findings.
Do not proceed to Finalize before every spawned agent returns; agents do not write files.

## Finalize

1. Dedupe against existing PR comments (PR Mode) — merge findings sharing a root cause with an already-open comment; don't restate them as new.
2. Refine every MED/LOW-confidence finding with one more targeted search: `UNCHANGED` (verified correct), `UPDATED` (new context improved it), or `INCORRECT` (delete it).
3. Verify against loaded guidelines: flag violations as `[GUIDELINE: <source> — <rule>]`; if a finding contradicts a guideline, the guideline wins per the Rule Precedence table — document the conflict.
4. Every surviving finding needs: `confirmed`/`likely` confidence, exact `file:line`, and an actionable fix (diff format). PR Mode also needs open-comment resolution checked (re-flag as unresolved if a prior comment's issue wasn't actually fixed).
5. Cap to the ~5-7 most impactful findings, prioritized HIGH severity first, then by domain weight from the Multi-Agent merge order above. Move lower-priority items to an "additional notes" section rather than dropping them silently.

## Finding Shape

Every surviving finding uses this shape, whether shown in chat or written to a document:

```text
[DOMAIN-1] title
Severity: HIGH|MED|LOW
Confidence: confirmed|likely|uncertain
Location: path:line
Evidence: exact read + proof lane
Impact: caller/user/data/contract consequence
Fix: minimal code direction or diff
```

Use `[SEC-1]`/`[BUG-1]`-style descriptive IDs, plain `1.`/`2.`, or lettered labels — never `#1`/`#2`/`#N` (GitHub auto-links `#<number>` to issues/PRs).

Continue with `workflow-pr-review-report.md` for report template, delivery rules, and checklist.

Validate: `node scripts/eval-research.mjs --case pr-local-review`.
