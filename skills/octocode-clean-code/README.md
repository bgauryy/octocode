# Octocode Clean Code

Cut dead weight from a codebase without changing observable behavior.

## Use when

- Shims, legacy stubs, re-exports, aliases, or compatibility adapters need removal.
- Duplicate logic, copy-pasted helpers, or redundant utility wrappers clutter the codebase.
- Patch regexes, monkey-patches, or always-true environment conditionals accumulate.
- Verbose comments, dead comment blocks, junk docs, or god documentation need trimming.
- Config files are bloated, contain redundant keys, or exceed sensible length.
- God files (one file doing multiple jobs) or god folders (one folder owning many domains) need splitting.
- Files are misplaced in the wrong layer or directory.

## Rules

- Prove zero external callers before any deletion — LSP callers and graph import edges both required.
- Never change behavior; flag any removal that requires a behavioral change.
- One consent-gated batch at a time; run the project's own checks after each batch.

## Workflow

```text
SCOPE → AUDIT → INVENTORY → TRIAGE → CONSENT → EXCISE → VERIFY
```

## Install

```bash
npx octocode skill install octocode-clean-code --platform codex
```

## Research trail

Sources consulted during creation: `references/references.md`.

## Maintainer verification

Run the `octocode-skills` review against this folder.
