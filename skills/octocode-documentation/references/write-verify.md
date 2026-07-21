# Write And Verify

Load for the outline gate, write pass, and post-write checks.

## Outline gate

Unless the user already approved targets this turn, present:

```text
Mode:     <agent-docs | human-docs | adr | codebase-pack>
Type:     <Diátaxis type or n/a>
Targets:  <paths>
Outline:  <TOC bullets>
Evidence: <modules/docs inspected>
Risks:    <gaps, overwrites>

1. Write  2. Adjust  3. Research more  4. Cancel
```

IF a target exists → THEN ask Overwrite, Diff first, Rename, Skip, or Cancel.

## Write

1. Load `agent-readable.md` (and the mode ref) if not already loaded.
2. Follow the approved outline; one Diátaxis type per human page.
3. Link related docs; put deep facts in the owning page, not in AGENTS.md.
4. Use durable module/doc pointers; skip large code blocks.
5. IF evidence is missing → THEN write "Not verified in repo" — never fabricate.
6. Match existing terminology and heading style.

## Verify

1. Commands/scripts named in the doc exist in manifests/CI/docs.
2. Linked paths exist.
3. No secrets or private URLs introduced.
4. agent-docs: within length budget; External References present; no README dump.
5. human-docs: single type; cross-links present; no code dumps.
6. ADRs: required sections present; linked from index when relevant.

IF verification fails → THEN fix or report residual risk. Do not claim completeness for gaps.
