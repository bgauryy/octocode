---
name: octocode-clean-agentic-code
description: "Use when behavior-preserving cleanup must remove dead exports, shims, aliases, duplicate logic, patch kludges, stale prose/config/schemas/dependencies/tests, misplaced or oversized files/folders, agent residue such as reinvention, scope creep, narration, type/lint suppressions, speculative abstraction, error masking, and test or grader gaming, or dated instruction cruft in prompts, AGENTS.md/CLAUDE.md, skills, and tool descriptions (verification rituals, emphasis boosters, scaffolds, stale few-shot, contradictory rules, dated model config). Triggers include clean up, remove legacy, dead code audit, god file, spaghetti code, unused deps, test hygiene, remove AI slop/metadata, prompt cruft, outdated instructions, and stale numbers. Not for feature work, behavioral refactors, or critique-only requests → octocode-roast."
---

# Octocode clean agentic code

tools: `npx octocode` / `octocode-mcp`
related-skill: `octocode-research`
output: `<workspace>/.octocode/` for workspace work | `<home>/.octocode/` when no workspace applies
routes: load a reference only when it changes the next action; otherwise keep the rule here.

Remove dead weight and agent residue without changing observable behavior.
Reports go to `<output>/octocode-clean-agentic-code/`; scratch goes to `<output>/tmp/octocode-clean-agentic-code/`. Chat-only findings stay in chat; source edits keep their named paths.

```mermaid
flowchart LR
    S["SCOPE"] --> AU["AUDIT"] --> I["INVENTORY"] --> T{"TRIAGE"}
    T -- "hides a failure" --> R["Report only"]
    T -- "spaghetti knot" --> K["Inventory, keep out of batch"]
    T -- "safe batch" --> C["CONSENT"] --> E["EXCISE"] --> V["VERIFY"]
    V -- "next batch" --> T
    S -. "at startup or when choosing a phase" .-> PB["cleanup-playbook.md"]
    AU -. "dead export, duplicate, patch kludge, junk prose" .-> SC["smell-catalog.md"]
    AU -. "agent-written: reinvention, scope creep, narration, audit order" .-> AD["agentic-defects.md"]
    AU -. "suppressions, one-implementation abstraction, annotation churn, scratch scripts" .-> AB["agentic-bloat.md"]
    AU -. "god file, misplaced layer, crossed phases, flag branches, deep nesting" .-> ST["structure.md"]
    AU -. "verbose comments, dead JSDoc, god docs, redundant config keys" .-> DC["doc-config-hygiene.md"]
    AU -. "narration, pasted probe output, provenance trails, stale counts" .-> DR["decision-residue.md"]
    AU -. "schema, type, or dependency redundancy" .-> DH["declaration-hygiene.md"]
    AU -. "prompt or tool-description cruft; before editing it" .-> IC["instruction-cruft.md"]
    AU -. "iteration files, untracked skips, rigid mocks, env-coupled tests; replacement test" .-> TH["test-hygiene.md"]
    R -. "error masking, null defaults, stubs, insecure deps, placeholder credentials" .-> AC["agentic-correctness.md"]
    R -. "weak oracles, co-edited assertions, patched graders, CI weakening" .-> TG["test-gaming.md"]
```
Caption: every batch loops through VERIFY; disguised failures and knots never enter a batch; dotted edges load a page in `references/`.
Pages (load each when its map edge fires): `references/cleanup-playbook.md` · `references/smell-catalog.md` · `references/agentic-defects.md` · `references/agentic-bloat.md` · `references/structure.md` · `references/doc-config-hygiene.md` · `references/decision-residue.md` · `references/declaration-hygiene.md` · `references/instruction-cruft.md` · `references/test-hygiene.md` · `references/agentic-correctness.md` · `references/test-gaming.md`.

## Lobby rules
- Before deleting an export or adapter, inspect its exact source, references, entrypoints, and config. Use LSP references for symbols and callers for callable relationships; AST topology supplies candidate file edges. Empty results do not exclude dynamic or external consumers.
- Never change behavior. For instruction text, behavior means every outcome, constraint, and contract the text decides. If removal requires a behavioral change, flag it and stop.
- Separate dead weight from code that disguises a failure; report the second class instead of deleting it.
- Use safe batches; run the repo's checks after each. Read their output, never a summary.
- Keep edits within the requested cleanup scope. Reuse existing authorization for that scope; ask only when a proposed deletion or behavior change exceeds it.
- Config cleanup that affects runtime behavior needs explicit consent. Never touch lockfiles, generated output, or build artifacts.
- Spaghetti is forbidden. Detect tangled control flow, and refuse any edit that creates or extends it: a new flag, nested branch, wrapper, or copied function. Report the knot and leave it out of the batch.

Related: `octocode-research` (symbol proof, callers, import graphs, blast radius) · `octocode-prompt-optimizer` (instruction rewrites that change intent) · `octocode-roast` (smell inventory) · `octocode-architect` (structural untangles) · `octocode-eval-benchmark` (metrics) · `octocode-skills` (folder changes). No scripts.
