# STE-80: explain in ASD-STE100, with diagrams for flows

Load when you explain something, or write a runbook, procedure, troubleshooting page, handoff, report, or agent instruction that a reader must act on. STE-80 is "80% of the way to" [ASD-STE100 Simplified Technical English](https://www.asd-ste100.org/) (Issue 9), the controlled language from aerospace maintenance: keep its writing rules, relax its dictionary. Its goal: each sentence has only one possible meaning.

Evidence: in one in-repo test (115 facts, five formats), STE-80 was the shortest (1.13x source) with 0 wrong facts; free prose had 1 wrong and 16 unsupported claims. Strict STE added about 10% words and no accuracy.

## Writing rules (from ASD-STE100)

1. Procedures: one instruction per sentence, imperative, 20 words or fewer. Put the condition first: "If the build fails, run `X`."
2. Descriptions: one topic per sentence, 25 words or fewer. Six sentences or fewer per paragraph, one topic per paragraph.
3. Active voice. Name the actor.
4. One word for one meaning, one meaning for one word. No synonyms for variety.
5. Noun clusters of three words or fewer: "the cache key for the request", not "request cache key hash value".
6. Keep articles and verbs; do not drop words to save space.
7. Do not use an -ing phrase to carry order: "Stop the server. Then delete the lock."
8. Use simple tenses. No idioms, slang, or figures of speech.
9. Relaxed dictionary: normal developer words are allowed. Commands, flags, paths, and product names are technical nouns; copy them exactly.
10. Add no advice, cause, or order that the source does not give. Mark an inference as an inference.

## Diagrams over dense sentences

- When the source states a flow, order, branch, loop, state change, or component relation, draw it as one Mermaid diagram instead of a paragraph.
- Keep it small: 12 nodes or fewer; split a big picture into an overview and details. Put conditions on edge labels.
- Under the diagram, write one caption line with its message. Do not repeat its edges in prose.
- Text keeps what a diagram cannot hold: thresholds, commands, IDs, owners, and the reason for a rule.
- Draw only edges and orders the source states. In the test, the diagram writer invented 7 edges or orders.
- A short linear chain stays inline: `build → test → publish`.

## Audience

| Reader | Signals | Write |
|---|---|---|
| Agent | prompt, `SKILL.md`, `AGENTS.md`, tool description, handoff, findings that another agent reads | STE-80 text. Flows as Mermaid source or an arrow chain. No HTML, images, or decoration. |
| Human | "explain", "how does X work", "walk me through", README, guide, onboarding, RFC, review | Lead with the diagram, then STE-80 steps; tables for comparisons. |
| Human, visual | "interactive", "page", "visual", "slides" | A single-file HTML explainer, only on request. It costs about 3x the tokens; keep it out of agent context. |

If the reader is unclear, write for the agent. That text also serves a human.

## Check

- Split each sentence over 25 words, or justify it.
- Replace each paragraph that walks through a flow with a diagram.
- Trace each claim to the source, or label it "Not verified in repo".

Next: word choice → `references/style-words.md`; steps → `references/style-structure.md`; image rules → `references/style-format.md`.
