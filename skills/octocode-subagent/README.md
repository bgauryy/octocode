# Octocode Subagent

Decide how to execute substantial work: solo, batched, delegated to tool-using workers, or offloaded to a local Ollama model.

## Use when

- Substantial work has independent streams that justify coordination cost.
- A specialist or fresh reviewer can improve evidence quality.
- Low-risk summarize, extract, classify, translate, draft, vision, or map-reduce work can run as a sealed local-model packet.

## Not for

- Routine edits or dependent sequences where one call handles everything → work directly
- Explanations or known reads that fit a single batched tool call
- Coordination without independent ownership lanes → keep work in the parent

## Workflow

**Tool-using workers:**
```text
FRAME → GATE → DECOMPOSE → ROUTE → PACKET → SPAWN → COORDINATE → VERIFY → SYNTHESIZE → CLEANUP → REPORT
```

**Local Ollama:**
```text
GATE → ROUTE → RUN → VERIFY → REPORT
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
