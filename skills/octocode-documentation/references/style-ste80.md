# STE-80: explain in ASD-STE100, with diagrams for flows

Load when you explain something, or write a runbook, procedure, troubleshooting page, handoff, report, or agent instruction that a reader must act on. STE-80 is "80% of the way to" [ASD-STE100 Simplified Technical English](https://www.asd-ste100.org/) (Issue 9): keep its writing rules, relax its dictionary. Goal: each sentence has only one possible meaning.

Evidence: in an in-repo test (115 facts), STE-80 was shortest with 0 wrong facts; free prose had 16 unsupported claims; strict STE added words, not accuracy.

## ASD-STE100 writing rules

1. Procedures use the imperative: "If the build fails, run `X`."
2. Descriptions: one topic per sentence, 25 words or fewer; split or justify a longer sentence. Six sentences or fewer per paragraph, one topic per paragraph.
3. Name the actor.
4. One word for one meaning, one meaning for one word. No synonyms for variety.
5. Noun clusters of three words or fewer: "the cache key for the request", not "request cache key hash value".
6. Keep articles and verbs; don't drop words to save space.
7. Don't carry order with an -ing phrase: "Stop the server. Then delete the lock."
8. Only present, past, and future tenses (no perfect or progressive). No idioms, slang, or figures of speech.
9. Relaxed dictionary: normal developer words are allowed. Copy commands, flags, paths, and product names exactly.
10. Add no advice, cause, or order the source doesn't give. Mark an inference as an inference.

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
