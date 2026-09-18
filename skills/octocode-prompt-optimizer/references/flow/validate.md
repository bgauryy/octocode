# VALIDATE

Load after FIX and before OUTPUT. Why: validate the complete draft before writing or presenting it.

## Required Checks

Apply the shared checks to every draft. Apply a domain check only when the target or changed behavior uses that domain; an irrelevant domain is skipped, not failed.

- [ ] Critical rules use proportionate enforcement; optional guidance stays optional.
- [ ] Every rule is applicable during generation: it names an observable action and the boundary against the behavior it is confused with, and executes without rereading its section.
- [ ] Each example pair is the smallest that separates the confused behaviors; a second example restating the same boundary is cut.
- [ ] Every sentence defines a distinction, sets a boundary, explains a consequence, or directs an action; no motivational, role-play, or decorative residue remains.
- [ ] No conflicting instructions, ambiguous actions/referents, filler, or duplicate rule owners.
- [ ] For a tool or MCP surface: each rule sits in its owning layer, shared descriptors have one name/type/meaning from a single definition, and no tool pair lacks a deciding condition.
- [ ] For a cross-app contract: canonical semantics have one owner, each adapter preserves native wire rules, and normal, invalid, denied, retry, cancellation, stale-version, and removal paths are tested where applicable.
- [ ] For MCP: the negotiated version, paginated `tools/list`, definition fields, schema-valid call/result, structured-content validation, catalog change signal, and untrusted annotations are handled where applicable.
- [ ] For runtime-assembled context: the executing surface, runtime-resolved dependency/version, active entrypoint/configuration, composition order, visibility, lifetime, persistence, and last observable model/tool input are evidenced; declared and installed dependencies are not mistaken for the running instance.
- [ ] For context: the current model limit and exact serialized occupancy are measured, output/reasoning capacity is reserved, and cached tokens are not mistaken for freed window space.
- [ ] For economics: current model/vendor prices and usage buckets feed cost per successful task; cache writes, reads, output/reasoning, retries, and paid tool calls are not omitted.
- [ ] For caching: a cold/warm/tail-change check confirms vendor telemetry, and every miss is classified before the prompt is changed.
- [ ] For frozen agent prompts: base version and digest match before dispatch; task-specific content is an append-only overlay; intentional base/tool changes create a reviewed version.
- [ ] Original intent, required branches, exact commands, and necessary frontmatter/metadata remain intact.
- [ ] Every intended branch has an explicit trigger, action, output, and recovery; branches do not overlap ambiguously.
- [ ] Expected outputs have concrete shapes; decision points have explicit routing such as IF/THEN or a decision table.
- [ ] Separate examples and reference data only where readers can mistake them for live instructions.
- [ ] Critical rules sit where the agent finds them; every tag closes and separates only real data.
- [ ] The before/after score is on record, and behavior justifies any material growth.

## Final questions

1. Can this execute reliably for every intended mode?
2. What is the weakest remaining branch or section?
3. Which sentence can a model read without changing what it does next? Cut it.
4. Did any edit change intent? The answer must be No.

Do not output after a failed check or force a valid multi-mode prompt into one path. Repair local failures in FIX; return to UNDERSTAND when intent changed or a material choice remains unresolved.

## Sources
- Anthropic, [Writing effective tools for AI agents](https://www.anthropic.com/engineering/writing-tools-for-agents) — use verifiable outcomes and measure errors, calls, and tokens alongside task success.

Next: when every check passes load `references/flow/output.md`; when a local repair remains return to `references/flow/fix.md`; when intent changed return to `references/flow/gates.md`; when a reliability claim still needs proof load `references/flow/evaluation-data.md`.
