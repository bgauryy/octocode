# Agent Residue

Load when cleaning a codebase an agent wrote or edited, or when you order an agent-residue audit or justify which class to hunt first. Measured prevalence, not intuition, sets the order.

Everything here is behavior-preserving and eligible for an excision batch. Correctness defects that need a behavioral decision live in `references/agentic-correctness.md` — classify first, because the two tiers have opposite protocols.

## Audit order

Hunt in descending measured prevalence; each earlier class makes later ones easier to see.

1. Duplication and reinvention: most measured; compounds silently.
2. Error masking: cheap to detect, highest severity per instance, report-only.
3. Test integrity and weak oracles: they invalidate the checks the audit depends on.
4. Scope-creep leftovers, annotation churn, oversized changes: strongest predictor of rejected agent work.
5. Dependency, supply-chain, and credential defects: low volume, unbounded blast radius.
6. Checker suppressions and speculative abstraction: they hide dead code from the tools that find it.
7. Narration residue and low-connectivity files: highest volume, lowest risk; batch last.

## Reinvention and parallel implementations

The dominant agent smell: new code that stands alone instead of joining the codebase.

| Signal | Query | Evidence bar |
|--------|-------|--------------|
| Self-contained algorithm (distance, parser, retry, deep-clone, date math) with no imports | `astSearch` match on the function body | A dependency in `package.json` or an internal module already provides it |
| Two modules exporting the same symbol names, disjoint consumer sets | `astTopology` dependents on both (beta; else `lspSearch` references) | `lspSearch` references prove which one is live |
| New file with zero dependents and few outgoing calls | `astTopology` dependents + deadCode candidates (beta; else `lspSearch` references + `localSearch`) | Reachability confirms it is unreferenced, rather than merely new |
| Third variant of one rule (validation, formatting, auth check) | `localSearch` lexical search on the rule's literals | All variants listed; canonical chosen before any delete |

An availability check that wrongly reports the original as absent is a common root cause — verify the check before deleting either copy, or the agent rebuilds it again.

## Scope-creep leftovers

| Signal | Verification required |
|--------|----------------------|
| Edits in files unrelated to the stated task | Change is not required by the task's call graph |
| Whole-file reformat mixed into a logic change | Reformat isolated; logic diff re-read on its own |
| Defensive guard added around code that cannot reach that state | `lspSearch` callers show the state is unreachable |
| Unreferenced config key, flag, or env var introduced alongside a feature | Zero readers anywhere in the repo |

## Narration and process residue


| Type | Remove when |
|------|------------|
| Change narration in source (`// changed from X to Y`, `// this should work`) | Always — the history owns this |
| Instructions aimed at an agent left in shipped code | Always — move to the repo's agent guide |
| Summary, plan, or handoff markdown dropped into a source directory | Content is superseded or duplicated by the real docs |
| Sibling files named `*-v2`, `*-final`, `*-new`, `*-fixed`, or dated | Base file is canonical; sibling adds no unique path |

## Regex where structure exists

Treat as low confidence: a generated regex over code, JSON, or another structured format is a candidate, not a defect, and some are correct and load-bearing.

| Signal | Before proposing removal |
|--------|--------------------------|
| Regex parsing a language, config format, or tool output | A real parser, AST query, or `lspSearch` covers the same need |
| Unbounded nested quantifier (`(a+)+`, `(.*)*`) | Catastrophic-backtracking risk stated with the input that triggers it |

Replacing a regex changes behavior at the edges — propose it as a follow-up, not an excision.

## Base rates: sources and what they set here

| Finding (source) | Application |
|---|---|
| 5+ line duplicate blocks +81% since 2023; copy/paste 15.7% vs 3.8% refactored; cross-file connectivity −35%; error masks +47% ([GitClear](https://www.gitclear.com/the_ai_code_quality_maintainability_gap)) | Duplication first; a new file with zero dependents signals reinvention; masks accumulate |
| Duplicates are 23% of rejected agent PRs; unmerged changes touch more files ([MSR 2026](https://arxiv.org/abs/2601.15195)) | Rank duplicate work and oversized diffs above style debt |
| Agents suppress errors to get runnable code and reimplement libraries ([9 failure patterns](https://daplab.cs.columbia.edu/general/2026/01/08/9-critical-failure-patterns-of-coding-agents.html)) | Error-masking and reinvention classes |
| 80.2% of 86,156 agent test patches have a weak or missing oracle ([oracle study](https://arxiv.org/html/2606.18168v1)) | Inspect the oracle before trusting a passing agent test |
| 50.4% of agent PRs on tested code add no tests; 81–86% of agent `try/catch` run under no test ([coverage study](https://arxiv.org/html/2607.18057v1)) | Uncovered catch blocks are unproven, not dead |
| Test cheating up to 54%, mostly test edits; read-only or hidden tests drop it near zero ([ImpossibleBench](https://arxiv.org/html/2510.20270v1)) | Test-gaming signals; prevention beats cleanup |
| Reward hacking in 70–95% of runs despite instructions, incl. patched timers and graders ([METR](https://metr.org/blog/2025-06-05-recent-reward-hacking/)) | An instruction is not a control; check the diff |
| Agent TS PRs add `any` about 9× as often (2.16 vs 0.24 per PR) ([type-safety study](https://arxiv.org/html/2602.17955v1)) | Checker-suppression pass |
| Annotation changes are the top three agent refactoring types ([refactoring study](https://arxiv.org/html/2601.20160)) | Annotation-only hunks are measured scope creep |
| Duplicated literals, cognitive complexity, unused parameters lead Sonar issues in 1,210 agent PRs ([PR quality](https://arxiv.org/html/2601.20109)) | Lint-level residue to batch |
| 38.9% of agent PRs carry a security smell; 82.3% of those are supply-chain integrity ([security smells](https://arxiv.org/html/2607.12428v1)) | Pinning and insecure config are report-only |
| 19.7% of samples name a nonexistent package; 205k invented names, many recurring ([USENIX Security 2025](https://arxiv.org/abs/2406.10279)) | Resolve every dependency name against the registry |
| Security pass rate flat at 56%; Java 30%, Python 63% ([Veracode 2026](https://www.veracode.com/blog/2026-genai-code-security-report-ai-risk/)) | Language-weighted review attention |
| Patch tools misplace hunks or report success on unchanged files ([Codex 30946](https://github.com/openai/codex/issues/30946), [aider 3651](https://github.com/Aider-AI/aider/issues/3651)) | Verify the effect; watch indentation drift |
| A parallel subsystem copy built after a stale absence check ([claude-code 87532](https://github.com/anthropics/claude-code/issues/87532)) | Verify the availability check before deleting either copy |
| Fabricated "verified" claims, unrequested changes, conventions lost after compaction, committed credentials ([72956](https://github.com/anthropics/claude-code/issues/72956), [83531](https://github.com/anthropics/claude-code/issues/83531), [6354](https://github.com/anthropics/claude-code/issues/6354), [2142](https://github.com/anthropics/claude-code/issues/2142)) | Read the artifact, never the summary; instruction files do not replace a scan |

## Weak evidence: do not overclaim

- Agents choosing regex where a parser belongs: no direct study; only indirect policy that AI regex needs backtracking review ([OWASP CRS](https://github.com/coreruleset/coreruleset/blob/main/AI-CONTRIBUTIONS.md)).
- Invented package names in shipped repositories: rates are measured on suggestions; repository evidence is mitigation tooling, not confirmed commits.
- Benchmark rates are soft: about 7.8% of "plausible" SWE-bench patches are wrong ([patch study](https://arxiv.org/html/2503.15223v1)). Cite rates as priority signals, never as a per-repository prediction.

Next: for suppressions, speculative abstraction, and churn load `references/agentic-bloat.md`; for the report-only tier load `references/agentic-correctness.md`; for test-gaming signals load `references/test-gaming.md`; to run the phases load `references/cleanup-playbook.md`.
