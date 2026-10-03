# Write and verify

Load for the outline gate, write pass, and post-write checks.

## Outline gate

Template for the targets the lobby gate names:

```text
Mode:     <agent-docs | human-docs | adr | codebase-pack | style-pass>
Type:     <Diátaxis type or n/a>
Targets:  <paths>
Outline:  <TOC bullets>
Evidence: <modules/docs inspected>
Risks:    <gaps, overwrites>
```

Then write, adjust, research more, or cancel, as the reply says.

## Write

1. Load `references/evidence-research.md` § Agent-readable writing and the mode reference when the lobby rules do not settle a choice.
2. Follow the approved outline; match existing terminology and heading style.
3. Apply the `references/style-pass.md` defaults from the first draft.
4. For each lint finding, load the reference its message names.

## Verify

1. Mode checks: agent-docs and ADR per `references/modes.md`; style-pass changes each trace to a named rule.
2. API or tool docs: trace input schema → adapter arguments → result and evidence fields → executable `next` continuation. Check valid and invalid examples, separate static types from runtime validation, and label reliability claims measured or unmeasured. For multi-tool changes, compare shared fields and run a composition case not used while drafting.

Next: wording rules or a review → `references/style-pass.md`.
