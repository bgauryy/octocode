# Octocode Clean Agentic Code

Remove dead weight from a codebase without changing observable behavior.

Use when behavior-preserving cleanup must remove dead exports and stubs, structural bloat, agent residue and bloat, instruction cruft, or test debt. Detect spaghetti and refuse cleanup that extends it. Report disguised failures (error masking, gamed tests, placeholder credentials) instead of deleting them.

Not for: behavior-changing refactors → `octocode-architect`; callers and blast radius → `octocode-research`; instruction rewrites that change intent → `octocode-prompt-optimizer`; bug fixes.

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
