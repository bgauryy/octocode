Load when cleaning or auditing code an agent wrote or edited.

# Agent defects

## Audit order

Prioritize reachable wrong results, masked failures, weakened tests and exposed credentials by impact. Then inspect duplication, scope creep, dependency defects and [unsupported suppressions](code-smells.md#checker-suppressions). Treat narration and low-connectivity files as candidates, not automatic deletions.

## Reinvention

Before deleting either copy, verify the availability check that caused it.

| Signal | Check / action |
|---|---|
| Self-contained algorithm (distance, parser, retry, deep-clone, date math), no imports: `astSearch` the body | a dependency or internal module provides it. |
| Same names exported to disjoint consumers: graphQuery `dependents` of the file after `graphBuild` (else `lspSearch` references) | `lspSearch` references prove which is live. |
| New file, zero dependents, few outgoing calls (graph rows are leads; confirm with `lspSearch` references): graphQuery `dependents` + `issues` with `detectors: ["unreachable-file","unused-export"]` (else `lspSearch` + `localSearch`) | unreferenced, not only new. |
| Parallel variants of one rule (validation, formatting, auth check): `localSearch` its literals | compare their meanings and consumers before choosing a canonical owner. |
| Same capability twice (two HTTP clients, loggers or date libraries; callbacks beside promises in one module): manifests + `localSearch` imports | one is canonical for the project; migrate callers in a separate batch. |
| Call to a method, option or flag the pinned dependency does not have: typecheck, `lspSearch` definition, or the dependency's source at the locked version (`artifactSearch` → `ghSearchCode`) | absent at that version: report it as a defect, never stub it. |

## Residue to remove

[scan-signals](../scripts/scan-signals.mjs) rows in category `residue` (`node <skill-folder>/scripts/scan-signals.mjs --diff <base> --category residue`) carry the literal forms; the rest need the diff.

| Signal | Check / action |
|---|---|
| Edits in task-unrelated files | Identify ownership and leave unrelated work in place. Only reverse edits from your own cleanup batch. |
| Whole-file reformat in a logic change | isolate it; re-read the logic diff alone. |
| Defensive guard on a supposedly unreachable state | Trace supported inputs, external callers and dynamic paths before treating the state as unreachable. |
| Config key, flag or environment variable added with a feature | Check runtime, deployment and external consumers before concluding it has no reader. |
| Change narration or agent-directed comments | Confirm no tool or documented workflow consumes the text. Keep instructions that still have an owner and purpose. |
| `handoff-doc` in a source directory | real docs supersede or duplicate it. |
| `iteration-sibling` | base canonical; sibling adds no unique path. |
| `debug-print` in non-CLI code | no diagnostic or observable-output contract needs it. |
| Patch on patch: a hunk that fixes the same change's earlier hunk, or an abandoned approach left beside its replacement behind a flag | history of those lines (`git log -L`) and `lspSearch` callers show one path is unreachable or superseded. |
| Chat or tool residue and placeholders in shipped text: `node <skill-folder>/scripts/residue-scan.mjs <paths>` ([residue-scan](../scripts/residue-scan.mjs)) | not a documented example or template; see [writing tells](writing-tells.md). |
| `todo-marker` | see TODOs in [text and config hygiene](text-config-hygiene.md#comments-and-documents). |

## Regex where structure exists

Low confidence until a parser check runs; replacement is a follow-up, never an excision.

- Regex parsing a language, config format, tool output → show a parser, AST query, or `lspSearch` covers it.
- Unbounded nested quantifier (`(a+)+`, `(.*)*`) → state the input causing catastrophic backtracking.

## Report, never delete

Removal would reveal a wrong result → [escalation protocol](#escalation-protocol). Error masking (category `error-masking` rows find the literal forms):

- `log-only-catch`: a handler that logs and continues, no rethrow or user signal. Its log line is part of the masked failure, not a debug print.
- `empty-catch`: a swallowed error with an empty handler.
- `empty-fallback`, or a default (`?.`, `?? <literal>`, `.get(k, default)`) on a value that is never legitimately absent, or a hardcoded sample response after a failed call.
- Success returned without checking the effect landed (unchanged bytes, rows, requests): highest severity.
- `not-implemented-stub`, or a `pass`, `return None`, `return {}` body, on a reachable path.
- Retry or sleep loop that swallows a failed call or has no bounded termination and failure signal.

Supply chain and credentials (verify):

- Dependency missing from the registry: `artifactSearch` with ecosystem `type` and exact `packageName` is empty (provider errors prove nothing); attacker-claimable, not a typo, until a maintainer confirms.
- Plausible, recent, near-zero-adoption package: compare with the package it imitates.
- Import absent from every manifest: deps-usage `undeclared`, or the graph `undeclared-dependency` detector; see [phantom dependency](code-smells.md#dependencies).
- `placeholder-credential`, or a placeholder URL for a real integration: never ran against a real credential.
- `credential` (scan-signals prints it redacted; fixtures are scanned for it too): redact, report for rotation; history removal is separate.
- `mutable-ref` (branch or major-only action ref, untagged or latest image, `*`/`latest` version, unpinned global install): record the intended version with the owner.
- `insecure-setting` (TLS verification off, privileged container, world-writable mode, write-all token): the owner picks the secure value.

## Escalation protocol

1. Report blocking defects in SBAR: masked failure and location, intended behavior, affected consumers and recommended repair. Use existing evidence; request more investigation only for a material gap.
2. Keep it out of every batch, approved ones included.
3. Confirmed test-integrity finding → clean the test per [tests](tests.md) only after the bug is fixed.

## Prevention

- Weight dependency review by ecosystem; instruction files do not replace a scan.
- After a patch tool edits files, verify its effect and indentation.
