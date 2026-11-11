Load when writing a paste-ready task prompt or when several questions could change a decision.

# Choose questions from several directions

Which answer would change what happens next? For a release, product may ask what the user was promised, engineering what breaks on retry, operations whether rollback restores data, and a skeptic what would disprove the leading explanation. Choose the cheapest observation that separates consequential alternatives. Merge questions answered by the same check. Inspect available evidence first. Ask the user only about unresolved choices they own. Stop when another answer would leave the action unchanged.

## Specify the input and output

Name the decision, relevant input and smallest usable result. For “improve,” specify the KPI, comparable baseline and a guardrail. Put every acceptance condition in the body. Preserve requested evidence and exact formats.

## Labels on copyable task prompts

Give each authored standalone task prompt or meta-prompt its own `text` fence. End it with exactly one `labels: ` line of one to five distinct, comma-separated cues. Replace an existing footer when revising. Leave verbatim submissions and host or machine formats intact.

Choose a cue for the **additional check** it suggests. In the first example, the labels suggest checking duplicated policy, dependency direction and repeated I/O. In the second, a practitioner’s name can cue inspection of an actual failing input. Narrow a name if its intended lens is unclear. Consider other failures that could overturn the result.

```text
Reduce modules touched per policy change. Preserve historical totals. Return the smallest refactor and its test.
labels: clean code, dependency inversion, efficiency
```

```text
Cut context tokens. Preserve verified policy answers on the same tasks.
labels: karpathy, attention, context management
```

For prompt work, `prompt engineering` cues a wording or format check. `context engineering` cues inspection of retrieved, retained or truncated input. To test a cue, compare the same cases with it omitted, labeled and made explicit in the body. Count useful findings, false alarms and input/output tokens including retries per verified completion. These comparisons measure observable behavior. They cannot identify which neurons activated.
