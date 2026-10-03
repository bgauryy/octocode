# Evidence research and agent-readable docs

Load when gathering or verifying repository facts before or after writing, and before WRITE for any audience (§ Agent-readable writing).

## Checklist

1. Orient on the project root and `<workspace>/docs/` (structure + find files).
2. Inventory README, CONTRIBUTING, AGENTS.md, ADRs, SECURITY, CI, manifests.
3. Collect commands only from manifests/Makefiles/CI.
4. Locate behavior by module/path search — entrypoints, config keys, public contracts.
5. Read focused slices; avoid whole large files.
6. Mark anything unfound as unverified.

## Durable evidence

- Prefer package/module paths, entry files, and doc links over `file:line` citations. Use line anchors only for short-lived debugging notes, not standing documentation.
- Describe ownership, contracts, and behavior ("auth token rotation under `packages/mcp-host` services") instead of pasting implementations.
- Name manifest scripts; do not embed large shell programs.
- Spot-check symbols with lexical or structural search, read the observed source exactly, then use LSP only when semantic identity or references matter. Anchored LSP needs a real URI plus either a zero-based UTF-16 `position` or an exact symbol name with a 1-based `lineHint`; inspect the live schema before an unfamiliar call.

## Anti-hallucination

- Assert only verified commands, paths, APIs, and env names.
- After about three targeted searches without a hit → mark unresolved and continue.
- Prefer "Not verified in repository" over plausible filler.
- IF code and an existing doc disagree → THEN trust code for the fact. Fix or flag the doc, and avoid ephemeral details.

## Claim map (before assertive prose)

| Claim | Evidence (path/module/doc) | Status |
|-------|----------------------------|--------|
| … | … | answered / partial / missing |

Only answered (careful partial) claims become firm docs.

## Agent-readable writing

- Density: lead with the fact or rule; cut intros, slogans, and restated README prose. One idea per bullet; tables for command/path maps. Prefer short pages that link deeper pages over one long page.
- Cross-refs: link related docs from new or updated pages (parent index, sibling how-to/reference, ADRs). `AGENTS.md` needs an External References (or Docs) table to the real sources of truth. Use repository-relative paths in backticks or markdown links — not vague "see docs". When two pages overlap, keep one owner; link from the other.
- No code dumps: skip multi-block source pastes, full configs, and long JSON/YAML in docs. Allowed: one short command or signature when the reader must copy-paste it; otherwise link the file. Keep examples minimal; point to tests or source for the full story.
- Agent comprehension check before finish: links and bullets must answer "where do I look?" and "what must I not break?". IF either answer is missing → THEN add refs or cut noise.

Next: outline gate, write steps, and post-write checks → `references/write-verify.md`; wording and formatting rules → `references/style-pass.md`.
