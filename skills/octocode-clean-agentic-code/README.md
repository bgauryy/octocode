# Octocode Clean Agentic Code

Remove dead weight from a codebase without changing observable behavior.

## Use when

- **Dead exports / stubs** — shims, aliases, re-exports, legacy adapters, duplicate helpers, or monkey-patches that are no longer used.
- **Structural bloat** — god files or folders, misplaced layers, oversized config, or junk documentation.
- **Spaghetti** — tangled control flow. Detect it, and refuse any cleanup that would extend the knot.
- **Agent residue** — reinvented imports, parallel subsystem implementations, scope-creep edits, or change-narration comments a coding agent left behind.
- **Disguised failures** — error-masking catch blocks, tests special-cased to pass, placeholder credentials, or artifacts that record process instead of decisions.

## Not for

- Behavior-changing refactors or architecture improvements → `octocode-architect`
- Finding callers or blast-radius analysis before cleanup → `octocode-research`
- Fixing bugs (removing the disguise reveals the defect; report it, don’t delete it)

## Commitments

- Confirm every removal against exact source, callers, entrypoints, config, and project checks.
- Never change behavior. Flag any removal that requires a behavioral change.
- Keep batches reviewable and within existing authorization.
- Report defects that disguise failures — never silently delete them.

## Workflow

```text
SCOPE → AUDIT → INVENTORY → TRIAGE → CONSENT → EXCISE → VERIFY
```

## Install

```bash
npx -y octocode skill install octocode-clean-agentic-code
```

## Maintainer verification

Run the `octocode-skills` review against this folder.
