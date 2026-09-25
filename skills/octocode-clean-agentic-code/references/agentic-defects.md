# Agent Residue

Load when cleaning a codebase an agent wrote or edited. Why: agent-authored junk has different signatures than human debt — reinvention beats reuse, and the leftovers hide in new files rather than old ones.

Everything here is behavior-preserving and eligible for an excision batch. Correctness defects that need a behavioral decision live in `references/agentic-correctness.md` — classify first, because the two tiers have opposite protocols. For prevalence and priority order, load `references/agentic-evidence.md`.

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

Next: for the report-only tier load `references/agentic-correctness.md`; for base rates and priority order load `references/agentic-evidence.md`; to run the phases load `references/cleanup-playbook.md`.
