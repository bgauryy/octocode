# Write and verify

Load for the outline gate, write pass, and post-write checks.

## Outline gate

Unless the requester already approved targets this turn, present:

```text
Mode:     <agent-docs | human-docs | adr | codebase-pack | style-pass>
Type:     <Diátaxis type or n/a>
Targets:  <paths>
Outline:  <TOC bullets>
Evidence: <modules/docs inspected>
Risks:    <gaps, overwrites>

1. Write  2. Adjust  3. Research more  4. Cancel
```

IF a target exists → THEN ask Overwrite, Diff first, Rename, Skip, or Cancel.

## Write

1. Load `references/agent-readable.md` (and the mode ref) if not already loaded.
2. Follow the approved outline; one Diátaxis type per human page; match existing terminology and heading style.
3. Link related docs; put deep facts in the owning page, not in AGENTS.md.
4. Use durable module/doc pointers; skip large code blocks.
5. IF evidence is missing → THEN write "Not verified in repository" — never fabricate.
6. Write to the style defaults in `references/style-index.md` from the first draft; don't leave style for a cleanup pass.

## Style

1. Run `node scripts/style-lint.mjs <changed paths>`; zero ERROR before done. It reads Markdown only, so lint clean isn't style clean — hand-check docstrings, HTML, and UI strings against the same references before you call the pass done.
2. Load the reference named in each finding (`references/style-index.md` maps them); fix or justify every hit.
3. Word-level disputes: look the term up in `assets/google-word-list.tsv` and quote the guidance.
4. Wording only — IF a fix needs a fact you haven't verified → THEN flag it instead of guessing.

## Verify

1. Commands/scripts named in the doc exist in manifests/CI/docs.
2. Linked paths exist.
3. No secrets or private URLs introduced.
4. agent-docs: within length budget; External References present; no README dump.
5. human-docs: single type; cross-links present; no code dumps.
6. ADRs: required sections present; linked from index when relevant.
7. style-pass: every change traces to a named rule; no claim changed; lint clean or residual hits explained.

IF verification fails → THEN fix or report residual risk. Do not claim completeness for gaps.

Next: the rule behind any wording change → `references/style-index.md`; a review someone else acts on → `references/style-review.md`.
