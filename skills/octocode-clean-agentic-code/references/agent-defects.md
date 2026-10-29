Load when cleaning or auditing code an agent wrote or edited.

# Agent defects

## Audit order

1. Duplication and reinvention.
2. Error masking: highest severity, report-only.
3. Test integrity, weak oracles ([tests](tests.md)).
4. Scope-creep leftovers, annotation churn, oversized changes.
5. Dependency, supply-chain, credential defects.
6. Suppressions, speculative abstraction ([code-smells](code-smells.md#checker-suppressions)).
7. Narration residue, low-connectivity files: batch last.

## Reinvention

Before deleting either copy, verify the availability check that caused it.

- Self-contained algorithm (distance, parser, retry, deep-clone, date math), no imports: `astSearch` the body → a dependency or internal module provides it.
- Same names exported to disjoint consumers: `npx octocode graph query dependents <file>` (CLI; else `lspSearch` references) → `lspSearch` references prove which is live.
- New file, zero dependents, few outgoing calls: `graph query dependents` + `issues --detector unreachable-file,unused-export` (CLI; else `lspSearch` + `localSearch`) → unreferenced, not only new.
- Third variant of one rule (validation, formatting, auth check): `localSearch` its literals → list all variants; choose canonical first.

## Residue to remove

- Edits in task-unrelated files → off the task's call graph.
- Whole-file reformat in a logic change → isolate it; re-read the logic diff alone.
- Defensive guard on an unreachable state → `lspSearch` callers prove it.
- Config key, flag, env var added with a feature → zero readers.
- Change narration (`// changed from X to Y`, `// this should work`), agent-directed instructions in shipped code → always (instructions move to the agent guide).
- Summary, plan, handoff markdown in a source directory → real docs supersede or duplicate it.
- Siblings `*-v2`, `*-final`, `*-new`, `*-fixed`, dated → base canonical; sibling adds no unique path.
- Debug prints (`console.log`, `print`, `dbg!`, `println!`) in non-CLI code → no diagnostic or observable-output contract needs them.

## Regex where structure exists

Low confidence until a parser check runs; replacement is a follow-up, never an excision.

- Regex parsing a language, config format, tool output → show a parser, AST query, or `lspSearch` covers it.
- Unbounded nested quantifier (`(a+)+`, `(.*)*`) → state the input causing catastrophic backtracking.

## Report, never delete

Removal would reveal a wrong result → [escalation protocol](#escalation-protocol). Error masking:

- `catch` that logs and continues, no rethrow or user signal.
- `except: pass`, empty `catch {}`, `.catch(() => {})`.
- Failed-call fallback (`?? 'default'`, hardcoded sample response).
- Success returned without checking the effect landed (unchanged bytes, rows, requests): highest severity.
- Default on a non-null type (`?.`, `?? <literal>`, `|| {}`, `.get(k, default)` on a required key).
- Stub (`pass`, `return None`, `return {}`, `throw new Error("not implemented")`) on a reachable path.
- Retry or sleep loop that swallows a failed call or has no bounded termination and failure signal.

Supply chain and credentials (verify):

- Dependency missing from the registry: `artifactSearch` with ecosystem `type` and exact `packageName` is empty (provider errors prove nothing); attacker-claimable, not a typo, until a maintainer confirms.
- Plausible, recent, near-zero-adoption package: compare with the package it imitates.
- Import absent from every manifest: [phantom dependency](code-smells.md#dependencies).
- Placeholder key, token, URL for a real integration: never ran against a real credential.
- Credential in version control: redact, report for rotation; history removal is separate.
- Mutable ref (`uses: x@main`/`@v1`, `FROM image:latest`, `"*"`/`"latest"`, unpinned global installs): record the intended version with the owner.
- Insecure setting (`verify=False`, `rejectUnauthorized: false`, `NODE_TLS_REJECT_UNAUTHORIZED`, `privileged: true`, `chmod 777`, `permissions: write-all`): the owner picks the secure value.

## Escalation protocol

1. SBAR blocker, even on cleanup-only requests: situation (file, line, class, exact failure disguised); background (correct behavior, callers depending on the current one); assessment (blast radius via `octocode-research`); recommendation (request a fix through normal development).
2. Keep it out of every batch, approved ones included.
3. Confirmed test-integrity finding → clean the test per [tests](tests.md) only after the bug is fixed.

## Prevention

- Weight dependency review by ecosystem; instruction files do not replace a scan.
- After a patch tool edits files, verify its effect and indentation.
