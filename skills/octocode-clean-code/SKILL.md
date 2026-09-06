---
name: octocode-clean-code
description: "Use when a codebase needs dead weight cut: shims, legacy stubs, re-exports, aliases, duplicate logic, patch regexes, verbose comments, junk docs, bloated config, god files, god folders, misplaced files, redundant schemas, duplicate type aliases, stale protocol stubs, unused dependencies, or version misalignment. Trigger phrases include clean up, remove legacy, remove shims, remove re-exports, dead code audit, god file, clean dependencies, unused deps, or dependency alignment. Not for feature work or behavioral refactors."
---

# Octocode Clean Code

Cut dead weight — shims, re-exports, duplicates, patch kludges, junk prose, redundant schemas, dependency junk, oversized config, god files, and misplaced files — without changing observable behavior.

Flow: `SCOPE → AUDIT → INVENTORY → TRIAGE → CONSENT → EXCISE → VERIFY`.

Workspace output contract: chat-only findings stay in chat. Saved inventory reports default to `<workspace>/.octocode/octocode-clean-code/`; scratch evidence uses `<workspace>/.octocode/tmp/octocode-clean-code/`. Source mutations keep their named paths. Never fall back to a user-level Octocode home for artifacts.

## Lobby rules
- Prove zero external callers before deleting any shim, alias, re-export, or adapter. LSP callers + graph import edges are both required.
- Never change behavior. If removal requires a behavioral change, flag it and stop.
- Batch into safe increments; run the project's own checks after each batch.
- Respect explicit user scope; only widen to the full repository when no scope is given or user approves.
- One excision batch per consent gate; do not pre-empt by removing anything unapproved.
- Config hygiene changes affecting runtime behavior require explicit consent.
- Never touch lock files, generated output, or build artifacts.

## Smell classes

| Class | Examples |
|-------|----------|
| Dead exports | re-exports, barrel aliases, legacy adapter shims, compatibility stubs |
| Duplicate logic | copy-pasted blocks, near-identical helpers, redundant utility wrappers |
| Patch kludges | inline regex fixups, monkey-patches, always-true environment conditionals |
| Junk prose | syntax-narration comments, dead comment blocks, god documentation, stale TODOs |
| Schema / type redundancy | type aliases that just re-name, duplicate interfaces, stale protocol stubs, redundant enums |
| Dependency junk | unused deps, duplicate declarations, version misalignment, phantom deps, workspace-protocol violations |

## Smart routes — load only what the current step needs
- When starting the cleanup run or choosing between phases (SCOPE, AUDIT, INVENTORY, TRIAGE, CONSENT, EXCISE, VERIFY), load `references/cleanup-playbook.md` — per-phase run with per-class audit queries, inventory table, and check contract.
- When classifying a smell as shim, re-export, duplicate, alias, or patch kludge, load `references/smell-catalog.md` — full taxonomy with detection queries and confidence rules for each class.
- When evaluating file placement, folder cohesion, god-file size, or god-folder concerns, load `references/hierarchy-rules.md` — one-file-one-concern, one-folder-one-domain, size limits, and move protocol.
- When reviewing inline comments or documentation files for verbosity, dead prose, or god-doc patterns, load `references/doc-hygiene.md` — cut/keep rules for comments, JSDoc, and documentation files.
- When inspecting config files for length, redundant keys, or misplaced settings, load `references/config-hygiene.md` — config hygiene rules, type-specific length limits, and consent gate.
- When reviewing type definitions, interfaces, enums, schemas, or protocol shapes for redundancy or aliasing, load `references/schema-hygiene.md` — type-alias rules, duplicate interface detection, and protocol stub evidence bar.
- When auditing package.json files for unused, duplicate, misaligned, or phantom dependencies across the repo, load `references/dependency-hygiene.md` — unused-dep checks, version alignment, workspace-protocol rules, and consent gate.
- When symbol proof, caller lists, import graphs, or structural search are needed, load `references/octocode.md` — tool routing for LSP, graph, and structural queries.

## Related routes
- Use `octocode-research` for blast-radius mapping before deletions; `octocode-roast` for a blunt smell inventory; `octocode-eval-benchmark` to measure before/after metrics.
- Use `octocode-skills` when changing this skill folder.

## Scripts
None — instruction-only. Proof uses `octocode-research` and the project's own repository tools; verification runs the project's own test suite.
