# Evidence Research

Load when gathering or verifying repo facts before or after writing. Why: docs must track real behavior without hard-coding brittle details.

## Checklist

1. Orient on root and `docs/` (structure + find files).
2. Inventory README, CONTRIBUTING, AGENTS.md, ADRs, SECURITY, CI, manifests.
3. Collect commands only from manifests/Makefiles/CI.
4. Locate behavior by module/path search — entrypoints, config keys, public contracts.
5. Read focused slices; avoid whole large files.
6. Mark anything unfound as unverified.

## Durable evidence

- Prefer package/module paths and doc links over `file:line` citations.
- Use line anchors only for short-lived debugging notes, not standing documentation.
- Describe ownership ("auth token rotation under `packages/mcp-host` services") instead of pasting code.
- Spot-check symbols with search (then LSP if needed); search first so line hints are real.

## Anti-hallucination

- Assert only verified commands, paths, APIs, and env names.
- After about three targeted searches without a hit → mark unresolved and continue.
- Prefer "Not verified in repo" over plausible filler.
- IF code and an existing doc disagree → THEN trust code for the fact and flag the doc.

## Claim map (before assertive prose)

| Claim | Evidence (path/module/doc) | Status |
|-------|----------------------------|--------|
| … | … | answered / partial / missing |

Only answered (careful partial) claims become firm docs. See `write-verify.md` for post-write checks; `agent-readable.md` for citation style.
