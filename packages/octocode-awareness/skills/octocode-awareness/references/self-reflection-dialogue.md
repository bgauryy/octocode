# Self-Reflection Dialogue

Use role dialogue when an idea is important, fuzzy, risky, or creative enough that one straight-line answer may miss perspective.
Skip routine edits, simple status checks, and obvious verification. The goal is better thinking, not role-play.

## Pattern

Role dialogue is a bounded self-review loop:

```text
QUESTION -> ROLE A view -> ROLE B challenge -> SYNTHESIS -> VERIFY -> CAPTURE
```

Role dialogue can run with real subagents when the host and user allow delegation.
When subagents are unavailable, one agent can use named perspectives internally.
The output is one synthesized recommendation with dissent, evidence, and next checks.

## Good Role Pairs

| Pair | Use when | Expected tension |
|---|---|---|
| Tutor / Student | Learning, onboarding, explaining a new approach | clarity vs. confusion |
| Builder / Tester | Designing a feature or workflow | capability vs. failure modes |
| Supporter / Skeptic | Post-task reflection or ambiguous tradeoff | what worked vs. what is unverified |
| Historian / Futurist | Changing long-lived docs or memory | precedent vs. future fit |
| User Advocate / Maintainer | UX, docs, or agent workflow | usefulness vs. maintainability |
| Compression / Recall | Context-heavy tasks | brevity vs. missing evidence |

Use two roles by default. Add a third only when the user explicitly asks or the third role has a distinct job, such as security, migration, or evaluation.

## Awareness Surfaces

Before role dialogue:

```bash
octocode-awareness attend --workspace "$PWD" --query "<idea or risk>" --compact
octocode-awareness query workboard --workspace "$PWD" --format table --limit 20
```

During role dialogue, use signals or refinements only when another run needs to see the discussion:

```bash
octocode-awareness signal publish --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --kind question --subject "<question>"
octocode-awareness refinement set --agent-id "$OCTOCODE_AGENT_ID" --workspace "$PWD" --quality handoff --reasoning "<why unresolved>" --remember "<next role to consult>"
```

For post-task learning, prefer the built-in advisory duo. It returns supporter/skeptic prompts but does not store, score, or enforce the extra role output:

```bash
octocode-awareness reflect record \
  --agent-id "$OCTOCODE_AGENT_ID" \
  --workspace "$PWD" \
  --task "<task>" \
  --outcome partial \
  --lesson "<short reusable lesson>" \
  --judgment-note "<evidence checked + remaining uncertainty>" \
  --duo \
  --compact
```

After role dialogue:

- Record a durable memory only if the synthesis is reusable, evidence-backed, and scoped.
- Record a refinement if action remains.
- Keep raw debate out of long Markdown docs; summarize the conclusion and point to row IDs or files.

## Guardrails

- Do not invent a permanent persona. Roles are temporary thinking lenses.
- Do not treat agreement between roles as proof. Verify against code, tests, docs, command output, or user feedback.
- Do not store every dialogue turn. Capture the synthesis, open question, or verified lesson.
- Do not spawn subagents unless the user or host policy allows it; use an internal two-column review instead.
- Do not claim fake consensus. Preserve unresolved dissent when evidence is missing.
- Keep the packet small: one question, two roles, one pass, three to five bullets per role, one synthesis, one next check.
- Do not refresh `.octocode/` projections for ordinary brainstorms; publish only verified guidance that helps a future run.

## Output Shape

```text
Question: <idea/risk>
Roles: <A> vs <B>
Role A: <best argument, 2-4 bullets>
Role B: <challenge, 2-4 bullets>
Synthesis: <decision or sharpened hypothesis>
Dissent: <remaining objection or missing evidence>
Evidence to verify: <files/commands/memory ids>
Next: <one action>
Memory candidate: no | yes, after <verification>
Capture: none | reflect | memory | refinement | signal
```

Self-reflection dialogue turns "talking to yourself" into a useful social-intelligence loop: more perspective, less bloat, clearer verification.
