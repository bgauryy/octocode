# Evidence research and agent-readable docs

Load when gathering or verifying repository facts, and before WRITE for any audience (§ Agent-readable writing).

## Checklist

1. Orient on the project root and `<workspace>/docs/`.
2. Inventory README, CONTRIBUTING, AGENTS.md, ADRs, SECURITY, CI, manifests.
3. Locate behavior by module or path search: entrypoints, config keys, public contracts.
4. Read focused slices, not whole large files.

## Durable evidence

- Use line anchors only in short-lived debugging notes. Name manifest scripts; don't embed large shell programs.
- Spot-check symbols with lexical or structural search, read the source, then use LSP only when semantic identity or references matter. Inspect the live schema before an unfamiliar LSP call (`octocode-research` owns call details).

## Claim map (before assertive prose)

| Claim | Evidence (path/module/doc) | Status |
|-------|----------------------------|--------|
| … | … | answered / partial / missing |

Only answered (careful partial) claims become firm docs.

## Agent-readable writing

- No intros, slogans, or restated README prose. One idea per bullet; tables for command and path maps. Short pages that link deeper pages.
- Link from new or updated pages to the parent index, sibling how-to or reference, and ADRs; never a vague "see docs". When two pages overlap, keep one owner and link from the other.
- A code dump is a multi-block paste, full config, or long JSON or YAML. Give one short command or signature only when the reader must copy it; otherwise link the file, tests, or source.
- Before finishing, links and bullets must answer "where do I look?" and "what must I not break?"; if not, add references or cut noise.

Next: outline gate and checks → `references/write-verify.md`; wording → `references/style-pass.md`.
