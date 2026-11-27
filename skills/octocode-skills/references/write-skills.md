Load when changing a skill's trigger, instructions or resource layout. Start from the [entry template](template.md). For the name and description, read [skill name and description](skill-description.md); for a step that should be a script, [code over prose](code-over-prose.md).

# Write and improve skills

## Before editing

Define the recurring task, host, inputs, authority and checkable result. Reuse the existing owner; a one-off answer or repository rule may need no skill. Record the requests and expected results per [define the test](check-skills.md#1-define-the-test-before-editing).

Change the smallest canonical surface that fixes the task, then run [check skills](check-skills.md). Count a shortening as a gain only when the reader makes the same decisions with fewer required reads; text size alone does not show it.

## Lobby shape

The lobby is the body of `SKILL.md`. Give the reader a first useful action, branch conditions, essential constraints and a completion check. Resolve critical missing inputs from available context or ask only for the blocking information. State the load condition when a protocol must be read before acting; keep permission and safety boundaries visible before effects.

Use the shape the task needs: short prose or steps for a simple path, a table for exact choices, a diagram for topology or branching that is harder to explain in prose. Use no mandatory opening, mental-model section, diagram or line quota. Never hide necessary detail in dense rows or extra files to satisfy a size target, and never replace clear prose with a larger table or a decorative diagram.

| Skill's job | Structure that earns its place |
|---|---|
| Perform a procedure | Ordered actions, branch conditions, result check and recovery. |
| Answer or recommend | Relevant sources or criteria, deciding evidence, uncertainty and a stopping condition. |
| Supply domain rules or style | Situation → rule/action → check; examples only where they resolve ambiguity. |
| Run a tool or produce an artifact | Required inputs, verified invocation, effects, output location and result check. |

These rows adapt the one template; they are not extra templates. Combine two only when one skill does both jobs.

## Lobby and files

Keep SKILL.md and each document under `references/` and `output.md` at 50 lines or fewer (the checker warns with `skill-length` and `doc-length`); split a longer document by load condition, never by packing lines. Move independent branch detail to `references/<topic>.md` and route it from the entry by an observable condition: “For a scanned document, read [OCR recipe](references/<ocr-recipe>.md).” A reference resolves that branch; it does not send the reader through more indexes.

Use the [placement table](../SKILL.md#decide-placement). Name resources by task or domain so their contents are predictable. Keep task-critical branches one read from the entry. Split by independent load conditions; merge pages the same reader always needs together and repeated decisions into one owner; keep distinct recipes conditional.

Keep only knowledge the agent needs to act: condition → action → check. Remove duplicate explanations, motivational prose and generic model definitions. Translate a relevant lens into a deciding question; do not require the full catalog or private deliberation. Preserve exact commands, identifiers, dependencies, effects, permissions, exceptions, recovery, completion checks and evidence distinctions. Update inbound links and anchors when moving content.

Use relative links resolved from the containing file. Document each script beside its invocation as [code over prose](code-over-prose.md) step 3 lists, so the agent runs it without reading its implementation. For assets, state what consumes them and how to verify the resulting artifact. Bundle required resources and imports; neighboring skills are optional. Human setup belongs in a README only when it adds information the entry does not provide. Shared rules have one canonical owner; regenerate copies needed by standalone bundles.

Optional metadata and host files need an actual consumer. Verify host support, preserve required license/attribution and control fields, and avoid copying provider-specific keys into a generic template. Skills remain subordinate to host and user instructions; a skill cannot grant itself authority.

## Result contract

Keep a simple result contract inline even when references or executable helpers exist; the [placement table](../SKILL.md#decide-placement) says when `output.md` earns a separate read. In Octocode's collection, renderer/catalog consumers use a `## Saved report` section and bundled `references/report-output.md`. Other skills follow their actual consumer's format, dependencies and destination.

## Standards and limits

The [Agent Skills specification](https://agentskills.io/specification) owns format limits and optional fields (`license`, `compatibility`, string-map `metadata`, experimental `allowed-tools`); host support varies. For publication or advanced YAML, run its validator as [run checks](run-checks.md#3-run-structural-checks) describes.

[Official authoring guidance](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices) supports concise instructions, shallow references and real-task evaluations. Verify command and host claims against the installed version. Never remove safety or return success-shaped defaults merely to shorten a workflow.
