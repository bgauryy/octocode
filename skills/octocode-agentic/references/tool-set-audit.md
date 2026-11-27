Load when auditing a whole tool set for overlap and drift, or laying out skill discovery and loading.

# Tool set audit and skill loading

## Set audit

1. Inventory from the published view the client receives: a `mcp-surface.mjs capture` file ([measure](mcp-context.md#measure-the-served-surface)), not an author, enforcement or catalog view that also holds private fields. Record each tool's name, one-line job and the layer that states it. A job stated nowhere is unroutable.
2. Per close pair, write the deciding sentence; if you cannot, merge, rename or add the condition to both descriptions.
3. Compare shared fields (name, type, unit, default, requiredness, scope); generate equivalents from [one definition](tool-descriptions.md#unified-tool-contract); document real differences.
4. Check each pair for drift; a `Next` prerequisite must match the target's required fields. Class (symptom) → repair:
   - Split owner (one rule in two layers) → one owner; delete the copy.
   - Name drift (`path` vs `directory` for one input) → one canonical name.
   - Type drift (required here, optional there) → align or document.
   - Semantic drift (`page` 1-based here, byte offset there) → one meaning per name.
   - Enum drift (guidance suggests a rejected value) → validate against the schema.
   - Phantom next (missing tool, field or mode) → fix or delete it.
   - Phantom output key (a result key the tool never emits or spells differently) → name the emitted key.
   - Silent failure (disagree on what empty proves) → state it once in server instructions.
   - Authority drift (description implies a forbidden mutation) → server instructions win.

Saved audit: one [Findings](report-sections.md#findings) row per tool, with the job and overlaps in Finding, the drift class in Layer and the repair in Fix; then shared fields with whether one definition owns them, and contradictions needing an owner decision. An unexamined tool is a gap.

## Skills and progressive disclosure

An illustrative layout for a hypothetical architecture-review skill:

```text
architecture-review/
├─ SKILL.md          short entry: when to use, next action
├─ references/       detail for one task branch each
├─ scripts/          deterministic operations, run rather than read
└─ assets/           templates a step consumes
```

1. **Discover:** read the name and description to judge relevance.
2. **Activate:** read `SKILL.md` only when the task matches.
3. **Execute:** open a reference, script or template only when a step needs it.

A strong description says when to use the skill, what it distinguishes from neighbors, inputs and outputs; no skill should claim every engineering task. Remote or MCP-served skill content is [untrusted content](security.md#untrusted-content). Authoring and checking skills belongs to `octocode-skills`.

## Sources

- [Agent Skills specification](https://agentskills.io/specification)
