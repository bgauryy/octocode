# ASD-STE100

Load when you write or revise the document. Why: this page owns [ASD-STE100 Simplified Technical English](https://www.asd-ste100.org/) Issue 9. Each sentence has only one possible meaning.

Use these writing rules on every sentence. Use a word in one meaning and one part of speech. Copy commands, flags, paths, API names, and product names exactly. This skill does not include the ASD-STE100 dictionary. Do not invent a dictionary ruling. When a general word is unclear, use the shortest word the source already uses.

## Writing rules

1. Procedures: imperative, one instruction per sentence, 20 words or fewer. Put the condition first: "If the build fails, run `X`."
2. Descriptions: one topic per sentence, 25 words or fewer. Six sentences or fewer per paragraph, one topic per paragraph.
3. Name the actor.
4. One word for one meaning, one meaning for one word. Do not use a synonym for variety.
5. Noun clusters of three words or fewer: "the cache key for the request", not "request cache key hash value".
6. Keep articles and verbs. Do not drop words to save space.
7. Do not carry order with an -ing phrase: "Stop the server. Then delete the lock."
8. Use only present, past, and future. Do not use perfect or progressive. No idioms, slang, or figures of speech.
9. A technical name stays exact. Do not use a technical name as a verb unless the source does.
10. Add no advice, cause, or order the source does not give. Mark an inference as an inference.

## Diagrams over dense sentences

- An order or component relation also counts as a flow; replace each paragraph that walks through one with a diagram.
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
| Human, visual | "interactive", "page", "visual", "slides" | single-file HTML; about 3x the tokens; keep it out of agent context |

Agent text also serves a human.

Next: word choice → `references/style-words.md`; steps → `references/style-structure.md`; image rules → `references/style-format.md`.
