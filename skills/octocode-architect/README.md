# Octocode Architect

Analyze software architecture with exact code evidence, then make or verify the smallest safe change.

## Use when

- A change affects algorithms, boundaries, contracts, data/control flow, persisted state, or several consumers.
- You need dependency, cycle, reachability, dead-code, coupling, or blast-radius analysis.
- A maintainability or performance problem may justify a safe refactor.

## Not for

- Gathering evidence without a specific architecture decision → `octocode-research`
- Behavior-preserving cleanup of dead code and agent residue → `octocode-clean-agentic-code`
- Open-ended exploration before a decision is formed → `octocode-brainstorming`

## Workflow

```text
FRAME → MODEL → PROVE → CHANGE → VERIFY
```

Review-only tasks stop at evidence-backed findings. Authorized implementation tasks include the smallest verified refactor needed for the named quality goal.

## Install

```bash
npx -y octocode skill install octocode-architect
```

## Maintainer verification

```bash
node scripts/eval-architect.mjs --self-test
node scripts/eval-architect.mjs --json
```

Then run the `octocode-skills` review against this folder.
