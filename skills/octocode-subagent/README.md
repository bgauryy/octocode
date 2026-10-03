# Octocode Subagent

Decide how to run substantial work: solo, batched, delegated to tool-using workers, handed to a remote A2A peer, or offloaded to a local Ollama model.

Use when independent lanes justify delegation cost, a specialist or fresh reviewer improves evidence, or low-risk text work can run as a sealed local-model packet. Not for routine edits, dependent sequences, explanations, or known reads that fit one batched call.

## Workflow

```text
DECIDE (solo or delegate) → PICK worker kind (cloud subagent | local Ollama | A2A) → BRIEF → RUN → VERIFY / MERGE
```

The parent owns user intent, authority, integration, irreversible actions, and the final verdict. Worker outputs are claims until the parent verifies load-bearing anchors. Local Ollama receives no tools or secrets.

## Install

```bash
npx -y octocode skill install octocode-subagent
```

## Maintainer verification

```bash
node scripts/eval-contract.mjs
```

Then run the `octocode-skills` review against this folder.
