# Write and improve skills

Load when changing a skill's trigger, instructions or resource layout. Start from the [entry template](../SKILL.md#template).

## Work loop

Define the recurring task, host, inputs, authority and checkable result. Reuse the existing owner; a one-off answer or repository rule may need no skill. Before editing, record requests and expected selection/action/output: ordinary and implicit intent, missing input, near-neighbor, no-skill and failure/recovery. Keep baseline results when comparing behavior.

Trace discovery → entry → selected resource → result. Change the smallest canonical surface that fixes the task, then follow [validation](check-skills.md). Judge concision by useful decisions and required reads as well as text size.

## Lobby shape

The lobby is the body of `SKILL.md`. Give the reader a first useful action, branch conditions, essential constraints and a completion check. Resolve critical missing inputs from available context or ask only for the blocking information. State the load condition when a protocol must be read before acting; keep permission and safety boundaries visible before effects.

Use the shape the task needs: short prose or steps for a simple path, a table for exact choices, a diagram for topology or branching that is harder to explain in prose. Use no mandatory opening, mental-model section, diagram or line quota. Never hide necessary detail in dense rows or extra files to satisfy a size target.

| Skill's job | Structure that earns its place |
|---|---|
| Perform a procedure | Ordered actions, branch conditions, result check and recovery. |
| Answer or recommend | Relevant sources or criteria, deciding evidence, uncertainty and a stopping condition. |
| Supply domain rules or style | Situation → rule/action → check; examples only where they resolve ambiguity. |
| Run a tool or produce an artifact | Required inputs, verified invocation, effects, output location and result check. |

Combine these only when the task needs them. The template is the sole starting skeleton; these are adaptations, not extra mandatory templates.

## Discovery: name and description

The body cannot rescue a trigger the host never selects. Check the actual served listing, including stale overrides, against intended and neighboring requests.

- Match the stable folder name: 1–64 lowercase ASCII letters, digits and single hyphens; no leading/trailing hyphen.
- Begin the description with “Use when” and recognizable task language. State its distinctive result and only the neighbor boundary needed to avoid a collision. Avoid exhaustive keywords, setup steps and marketing.
- Respect the 1,024-character format limit; it is not a target. Public Octocode skills require one JSON-string `description:` line because the native build embeds it. Other hosts may accept other YAML forms.
- Test the description against the skill's actual actions. Include the distinguishing capability and essential neighbor boundary; keep procedure, commands and quality claims in the body.

## Lobby and files

Use the [placement table](../SKILL.md#decide-placement). Name resources by task or domain so their contents are predictable. Keep task-critical branches one read from the entry. Split by independent load conditions; merge pages the same reader always needs together. Add a contents list when a long reference is hard to scan.

Keep only knowledge the agent needs to act: condition → action → check. Remove duplicate explanations, motivational prose and generic model definitions. Translate a relevant lens into a deciding question; do not require the full catalog or private deliberation. Preserve exact commands, dependencies, effects, permissions, recovery and evidence distinctions. Update inbound links and anchors when moving content.

Use relative links resolved from the containing file. Distinguish running a script from reading its implementation. Bundle required resources and imports; neighboring skills are optional. Human setup belongs in a README only when it adds information the entry does not provide. Shared rules have one canonical owner; regenerate copies needed by standalone bundles.

For a multi-file skill, route by an observable condition: “For a scanned document, read [OCR recipe](references/<ocr-recipe>.md).” A reference should resolve that branch, not merely send the reader through more indexes. Keep script inputs, working directory, dependencies, effects and result/failure checks beside its invocation; run helpers without requiring their implementation in context. For assets, state what consumes them and how to verify the resulting artifact.

Optional metadata and host files need an actual consumer. Verify host support, preserve required license/attribution and control fields, and avoid copying provider-specific keys into a generic template. Skills remain subordinate to host and user instructions; a skill cannot grant itself authority.

## Minimal single-file template

Use the [canonical template](../SKILL.md#template), then walk a normal request and a failure or missing-input case. Add only the resources those tasks need. Keep simple result contracts inline even when references or executable helpers exist. A separate `output.md` is useful for a complex report schema. In Octocode's collection, renderer/catalog consumers use a `## Saved report` section and bundled `references/report-output.md`. Other skills follow their actual consumer's format, dependencies and destination.

## Standards and limits

The [Agent Skills specification](https://agentskills.io/specification) owns format limits and optional fields (`license`, `compatibility`, string-map `metadata`, experimental `allowed-tools`); host support varies. Use its YAML-aware validator for publication or advanced YAML.

[Official authoring guidance](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices) supports concise instructions, shallow references and real-task evaluations. Verify command and host claims against the installed version. Never remove safety or return success-shaped defaults merely to shorten a workflow.
