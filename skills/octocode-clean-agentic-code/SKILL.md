---
name: octocode-clean-agentic-code
description: "Use when a codebase needs dead weight cut: shims, legacy stubs, re-exports, aliases, duplicate logic, patch regexes, verbose comments, junk docs, bloated config, god files, god folders, misplaced files, redundant schemas, stale protocol stubs, unused dependencies, version misalignment, rigid mocks, skipped tests, or agent-introduced junk such as reinvented helpers, parallel subsystems, scope-creep leftovers, error-masking catch blocks, and special-cased tests. Also use it to strip decision residue from code, comments, skills, and docs: pasted probe output, provenance and research trails, process metadata, counts nobody re-derives, and restated facts. Trigger phrases include clean up, remove legacy, dead code audit, god file, unused deps, test hygiene, clean up after the agent, remove AI slop, remove metadata, or drop stale numbers. Not for feature work or behavioral refactors."
---

# Octocode clean agentic code

Cut dead weight — shims, re-exports, duplicates, patch kludges, junk prose, redundant schemas, dependency junk, oversized config, god files, misplaced files, and agent residue — without changing observable behavior.

Flow: `SCOPE → AUDIT → INVENTORY → TRIAGE → CONSENT → EXCISE → VERIFY`.

Workspace output contract: chat-only findings stay in chat. Saved inventory reports default to `<workspace>/.octocode/octocode-clean-agentic-code/`; scratch evidence uses `<workspace>/.octocode/tmp/octocode-clean-agentic-code/`. Source mutations keep their named paths. Never fall back to a user-level Octocode home for artifacts.

## Lobby rules
- Before deleting an export or adapter, inspect its exact source, applicable references, entrypoints, and configuration. Use LSP references for symbols and callers for callable relationships; AST topology supplies candidate file edges. Empty results do not exclude dynamic or external consumers.
- Never change behavior. If removal requires a behavioral change, flag it and stop.
- Separate dead weight from code that disguises a failure; report the second class instead of deleting it.
- Batch into safe increments; run the project's own checks after each batch. Read the checks' real output, never a summary of it.
- Keep edits within the requested cleanup scope. Reuse existing authorization for that scope; ask only when a proposed deletion or behavior change exceeds it.
- Config hygiene changes affecting runtime behavior require explicit consent. Never touch lock files, generated output, or build artifacts.

## Smell classes

| Class | Examples |
|-------|----------|
| Dead exports | re-exports, barrel aliases, legacy adapter shims, compatibility stubs |
| Duplicate logic | copy-pasted blocks, near-identical helpers, reinvented library code, parallel subsystems |
| Patch kludges | inline regex fixups, monkey-patches, always-true environment conditionals |
| Junk prose | syntax-narration comments, dead comment blocks, god documentation, stale TODOs, change narration |
| Decision residue | decision narration, pasted probe output, provenance trails, process metadata, stale counts, restated facts |
| Schema / type redundancy | type aliases that only rename, duplicate interfaces, stale protocol stubs, redundant enums |
| Dependency junk | unused deps, duplicate declarations, version misalignment, phantom deps, unresolvable names |
| Test debt | numbered/dated iteration files, skipped tests with no ticket, rigid mocks coupling to private internals, redundant stubs with no expect reference, environment-coupled tests, unused beforeEach/afterEach setup |
| Agent residue | scope-creep leftovers, zero-dependent new files, error-masking catch blocks, special-cased tests |

## Smart routes — load only what the current step needs
- When starting the cleanup run or choosing between phases (SCOPE, AUDIT, INVENTORY, TRIAGE, CONSENT, EXCISE, VERIFY), load `references/cleanup-playbook.md` — per-phase run with per-class audit queries, inventory table, and check contract.
- When classifying a smell as shim, re-export, duplicate, alias, or patch kludge, load `references/smell-catalog.md` — full taxonomy with detection queries and confidence rules for each class.
- When the target was written or edited by a coding agent, load `references/agentic-defects.md` — reinvention, scope-creep, and narration signatures that differ from human debt; when ordering that audit, load `references/agentic-evidence.md` — measured prevalence per class and the claims that stay unproven.
- When a smell hides a wrong result rather than dead weight, load `references/agentic-correctness.md` — the report-only tier with its escalation protocol.
- When evaluating file placement, folder cohesion, god-file size, or god-folder concerns, load `references/hierarchy-rules.md` — one-file-one-concern, one-folder-one-domain, size limits, and move protocol.
- When reviewing inline comments or documentation files for verbosity, dead prose, or god-doc patterns, load `references/doc-hygiene.md` — cut/keep rules for comments, JSDoc, and docs; when inspecting config files for length, redundant keys, or misplaced settings, load `references/config-hygiene.md` — length limits and consent gate.
- When reviewing type definitions, interfaces, enums, schemas, or protocol shapes for redundancy or aliasing, load `references/schema-hygiene.md` — type-alias rules and duplicate interface detection; when auditing package.json files for unused, duplicate, misaligned, or phantom dependencies, load `references/dependency-hygiene.md` — unused-dep checks, version alignment, and consent gate.
- When code, comments, skills, or docs record how a decision was made — probe output, provenance, process metadata, or counts nobody re-derives — load `references/decision-residue.md` — residue types, the number test, and what to keep.
- When the task involves removing numbered/dated iteration test files, skipped tests, rigid mocks, redundant stubs, or unused test setup, load `references/test-hygiene.md` — the test smell classes, the evidence each delete requires, and the excision protocol.
- When writing replacement tests after removing legacy or rigid ones (to recover lost coverage), load `references/test-quality.md` — isolation patterns, naming conventions, and the coverage replacement rule.
- When symbol proof, caller lists, import graphs, or structural search are needed, load `references/octocode.md` — tool routing for LSP, graph, and structural queries.

## Related routes
- Use `octocode-research` for blast-radius evidence before deletions; it owns the MCP/CLI workflow and live tool/grammar discovery. Use `octocode-roast` for a blunt smell inventory, `octocode-eval-benchmark` for before/after metrics, and `octocode-skills` when changing this folder.
- No scripts — proof uses `octocode-research` and the project's own repository tools; verification runs the project's own test suite.
