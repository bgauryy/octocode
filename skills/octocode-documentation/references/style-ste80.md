# ASD-STE100

Use when the request calls for controlled technical English. For formal [ASD-STE100](https://www.asd-ste100.org/) compliance, consult the applicable standard and dictionary; this summary is writing guidance, not a compliance check.

Use the following guidance where it improves clarity. Use a word in one meaning and one part of speech. Copy commands, flags, paths, API names, and product names exactly. This skill does not include the ASD-STE100 dictionary. Do not invent a dictionary ruling. When a general word is unclear, use the shortest word the source already uses.

## Writing rules

1. Procedures: use direct instructions and split steps that ask for different actions. Put the condition first: "If the build fails, run `X`."
2. Descriptions: keep one main point per sentence or paragraph; split dense explanations where the reader changes focus.
3. Name the actor.
4. One word for one meaning, one meaning for one word. Do not use a synonym for variety.
5. Unpack dense noun clusters: "the cache key for the request", not "request cache key hash value".
6. Keep articles and verbs. Do not drop words to save space.
7. Do not carry order with an -ing phrase: "Stop the server. Then delete the lock."
8. Prefer familiar tenses and literal wording for instructions.
9. A technical name stays exact. Do not use a technical name as a verb unless the source does.
10. Add no advice, cause, or order the source does not give. Mark an inference as an inference.

## Diagrams over dense sentences

- Use a diagram when an order, branch, or component relation is easier to understand visually.
- Split a big picture into overview and details. Conditions go on edge labels.
- One caption line under the diagram; don't repeat its edges in prose.
- Text keeps what a diagram can't: thresholds, commands, IDs, owners, and the reason for a rule.
- Draw only edges and orders the source states.
- A short linear chain stays inline: `build → test → publish`.

## Audience

| Reader | Signals | Extra rule |
|---|---|---|
| Agent | prompt, `SKILL.md`, `AGENTS.md`, tool description, handoff, findings another agent reads | no decoration |
| Human | "explain", "how does X work", "walk me through", README, guide, onboarding, RFC, review | tables for comparisons |
| Human, visual | "interactive", "page", "visual", "slides" | choose a format the reader can view and use |

Agent text also serves a human.

Next: word choice → `references/style-words.md`; steps → `references/style-structure.md`; image rules → `references/style-format.md`.
