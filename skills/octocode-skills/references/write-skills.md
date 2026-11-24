# Write and improve skills

Load when changing a skill's trigger, instructions or resource layout. Start from the [entry template](template.md).

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
- The description is the trigger (when to load) plus a concise motivation (what the skill does and why to load it instead of working alone). Put the trigger first; keep procedure in the body.
- Begin the description with “Load this skill first when” and the request as a user would phrase it. An agent reads only the description when choosing, so it answers two questions: which requests, and why load it instead of working alone. In host runs this lead beat “Use when” on held-out requests with no new false triggers.
- Put the requested deliverable first when skills overlap. A merge verdict belongs to PR review, a clickable dependency map to Graph View, and a written set of test cases to Test Plan. Separate neighbors by that deliverable and the intent wording; the description names no other skill and carries no “Not for …” clause. Put handoffs to neighbors in the body, beside the input branch they serve.
- For jobs agents believe they can do alone (research, debugging, docs), name the shortcut it replaces in the lead (“before searching or reading files yourself”); recheck no-skill requests for false triggers.
- Avoid exhaustive keywords, setup steps and marketing.
- Respect the 1,024-character format limit; it is not a target. Public Octocode skills require one JSON-string `description:` line because the native build embeds it. Other hosts may accept other YAML forms.
- Test the description against the skill's actual actions. Include the distinguishing capability; keep procedure, commands and quality claims in the body.

## Lobby and files

Keep SKILL.md at 50 lines or fewer (the checker warns with `skill-length`): first action, deciding conditions, safeguards and completion stay inline; independent branch detail moves to `references/<topic>.md`, linked from a “When → Read” row.

Use the [placement table](../SKILL.md#decide-placement). Name resources by task or domain so their contents are predictable. Keep task-critical branches one read from the entry. Split by independent load conditions; merge pages the same reader always needs together. Add a contents list when a long reference is hard to scan.

Keep only knowledge the agent needs to act: condition → action → check. Remove duplicate explanations, motivational prose and generic model definitions. Translate a relevant lens into a deciding question; do not require the full catalog or private deliberation. Preserve exact commands, dependencies, effects, permissions, recovery and evidence distinctions. Update inbound links and anchors when moving content.

Use relative links resolved from the containing file. Distinguish running a script from reading its implementation. Bundle required resources and imports; neighboring skills are optional. Human setup belongs in a README only when it adds information the entry does not provide. Shared rules have one canonical owner; regenerate copies needed by standalone bundles.

For a multi-file skill, route by an observable condition: “For a scanned document, read [OCR recipe](references/<ocr-recipe>.md).” A reference should resolve that branch, not merely send the reader through more indexes. Keep script inputs, working directory, dependencies, effects and result/failure checks beside its invocation; run helpers without requiring their implementation in context. For assets, state what consumes them and how to verify the resulting artifact.

Optional metadata and host files need an actual consumer. Verify host support, preserve required license/attribution and control fields, and avoid copying provider-specific keys into a generic template. Skills remain subordinate to host and user instructions; a skill cannot grant itself authority.

## Prefer code over prose

Text is for judgment: what to choose, when to stop, how to read a result. Code is for exact operations. When a step's result must be exactly right, recurs on every run, or takes more than one mechanical step, write a script and make the step call it; do not describe the algorithm for the agent to perform by hand.

| Use a script when the step… | Keep it as text when the step… |
|---|---|
| parses or validates JSON, YAML, schemas or tool output; extracts fields | chooses between options or judges meaning |
| counts, sums, sorts, deduplicates, diffs, hashes or does date math | writes, reviews or summarizes for a reader |
| walks pagination, retries, or merges and rewrites files | is one fixed command: give it verbatim |
| calls an HTTP API with a fixed request shape or reads a credential | is already done by a tool the host has (Octocode MCP tools, a linter) |
| checks that links, paths, anchors or a manifest exist and match | sets scope, permissions or a stop condition |

How:

1. Put it in `scripts/<verb-noun>.mjs`: Node ESM with built-ins only, or a dependency bundled into `scripts/` by the skill's own private `package.json` (see `skills/common/README.md`), so the installed skill runs with nothing else. Read credentials only through the skill's `keys.mjs` (`getKey`).
2. Give it `--help` with inputs, outputs, effects and exit codes. Print JSON on stdout; fail with a stable error code (exit 2 for invalid input, 1 for a failed run); never print a secret. Keep a test under `scripts/tests/`.
3. In the step that needs it, write the exact command, its working directory, what it changes and how to read its result and failures. Link the script. Do not restate its logic in prose; point to `--help` for options.
4. Replace the prose procedure with that call and run it. A script the text never routes is code the agent will not use: the checker warns on it.

## Minimal single-file template

Use the [canonical template](template.md), then walk a normal request and a failure or missing-input case. Add only the resources those tasks need. Keep simple result contracts inline even when references or executable helpers exist. A separate `output.md` is useful for a complex report schema. In Octocode's collection, renderer/catalog consumers use a `## Saved report` section and bundled `references/report-output.md`. Other skills follow their actual consumer's format, dependencies and destination.

## Standards and limits

The [Agent Skills specification](https://agentskills.io/specification) owns format limits and optional fields (`license`, `compatibility`, string-map `metadata`, experimental `allowed-tools`); host support varies. Use its YAML-aware validator for publication or advanced YAML.

[Official authoring guidance](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices) supports concise instructions, shallow references and real-task evaluations. Verify command and host claims against the installed version. Never remove safety or return success-shaped defaults merely to shorten a workflow.
