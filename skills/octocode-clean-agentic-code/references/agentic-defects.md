# Agent Residue

Load to clean code an agent wrote or edited, or to order an agent-residue audit. Measured prevalence sets the order. Everything here is behavior-preserving. Defects that need a behavioral decision belong to `references/agentic-correctness.md`; classify first, because the two tiers have opposite protocols.

## Audit order

1. Duplication and reinvention: most measured; compounds silently.
2. Error masking: cheap to detect, highest severity, report-only.
3. Test integrity and weak oracles: they invalidate the audit's own checks.
4. Scope-creep leftovers, annotation churn, oversized changes: strongest predictor of rejected agent work.
5. Dependency, supply-chain, and credential defects: low volume, unbounded blast radius.
6. Checker suppressions and speculative abstraction: they hide dead code from the tools that find it.
7. Narration residue and low-connectivity files: high volume, low risk; batch last.

## Reinvention and parallel implementations

| Signal | Query | Evidence bar |
|---|---|---|
| Self-contained algorithm (distance, parser, retry, deep-clone, date math) with no imports | `astSearch` on the body | A `package.json` dependency or internal module already provides it |
| Two modules export the same names to disjoint consumers | `astTopology` dependents on both (beta; else `lspSearch` references) | `lspSearch` references prove which is live |
| New file with zero dependents and few outgoing calls | `astTopology` dependents + deadCode (beta; else `lspSearch` references + `localSearch`) | Reachability shows it is unreferenced, not only new |
| Third variant of one rule (validation, formatting, auth check) | `localSearch` on the rule's literals | All variants listed; canonical chosen before any delete |

A wrong availability check often caused the copy: verify it before you delete either copy, or the agent rebuilds it.

## Scope-creep leftovers

| Signal | Required proof |
|---|---|
| Edits in files unrelated to the task | The task's call graph does not need them |
| Whole-file reformat mixed into a logic change | Reformat isolated; logic diff re-read alone |
| Defensive guard around a state the code cannot reach | `lspSearch` callers show it is unreachable |
| Unreferenced config key, flag, or env var added with a feature | Zero readers in the repo |

## Narration and process residue

| Type | Remove when |
|---|---|
| Change narration in source (`// changed from X to Y`, `// this should work`) | Always; history owns it |
| Agent-directed instructions in shipped code | Always; move them to the agent guide |
| Summary, plan, or handoff markdown in a source directory | Real docs supersede or duplicate it |
| Siblings named `*-v2`, `*-final`, `*-new`, `*-fixed`, or dated | Base file is canonical; sibling adds no unique path |

## Regex where structure exists

Low confidence: some generated regex over code, JSON, or tool output is correct and load-bearing. Propose a replacement as a follow-up, not an excision; it changes edge behavior.

| Signal | Before proposing removal |
|---|---|
| Regex parsing a language, config format, or tool output | A parser, AST query, or `lspSearch` covers the same need |
| Unbounded nested quantifier (`(a+)+`, `(.*)*`) | State the input that triggers catastrophic backtracking |

## Evidence

Rates set priority only, never a per-repository prediction. Sources: [GitClear](https://www.gitclear.com/the_ai_code_quality_maintainability_gap), [MSR 2026](https://arxiv.org/abs/2601.15195), [9 failure patterns](https://daplab.cs.columbia.edu/general/2026/01/08/9-critical-failure-patterns-of-coding-agents.html), [oracle study](https://arxiv.org/html/2606.18168v1), [coverage study](https://arxiv.org/html/2607.18057v1), [ImpossibleBench](https://arxiv.org/html/2510.20270v1), [METR](https://metr.org/blog/2025-06-05-recent-reward-hacking/), [type-safety](https://arxiv.org/html/2602.17955v1), [refactoring](https://arxiv.org/html/2601.20160), [PR quality](https://arxiv.org/html/2601.20109), [security smells](https://arxiv.org/html/2607.12428v1), [USENIX 2025](https://arxiv.org/abs/2406.10279), [Veracode 2026](https://www.veracode.com/blog/2026-genai-code-security-report-ai-risk/), [claude-code 87532](https://github.com/anthropics/claude-code/issues/87532), [72956](https://github.com/anthropics/claude-code/issues/72956), [83531](https://github.com/anthropics/claude-code/issues/83531), [6354](https://github.com/anthropics/claude-code/issues/6354), [2142](https://github.com/anthropics/claude-code/issues/2142).

- Test gaming: prevention beats cleanup.
- Weight dependency review by language.
- Verify the effect of a patch tool; watch indentation drift ([Codex 30946](https://github.com/openai/codex/issues/30946), [aider 3651](https://github.com/Aider-AI/aider/issues/3651)).
- Instruction files do not replace a scan.
- Weak evidence: regex-over-parser has only indirect policy ([OWASP CRS](https://github.com/coreruleset/coreruleset/blob/main/AI-CONTRIBUTIONS.md)); invented-package rates come from suggestions, not commits; about 7.8% of "plausible" SWE-bench patches are wrong ([patch study](https://arxiv.org/html/2503.15223v1)).

Next: suppressions and churn → `references/agentic-bloat.md`; test gaming → `references/test-gaming.md`; batch → `references/cleanup-playbook.md`.
