Load when several questions could change a decision or when writing a reusable task prompt.

# Choose questions from several directions

Identify the decision and ask which missing answer could change the next action. Inspect available evidence first; merge questions answered by the same check. Use distinct perspectives when useful: promised outcome, retry failure, recovery, or the strongest disproof. Choose the cheapest observation that separates consequential alternatives. Ask the user only about unresolved choices they own. Stop when another answer would leave the action unchanged.

## Specify the input and output

Name the relevant input, authorized action, smallest usable result and observable acceptance. For an improvement claim, state the metric, comparable baseline and quality guardrail. Preserve required evidence and exact receiver formats. Put material checks in the prompt itself rather than relying on a model name or label to imply them.

## Labels on copyable task prompts

Use a separate `text` fence for a standalone prompt when it helps copying. Add a labels footer only if requested or required by the receiver's contract; preserve that contract and verbatim submissions. Ordinary prompts need no footer.

```text
Reduce modules touched per policy change. Preserve historical totals.
Return the smallest refactor, consumer evidence and regression-check result.
```

To measure whether wording or cues help, compare the same cases with and without them. Count verified outcomes, false alarms and tokens including retries. An editorial rewrite or a cue's name does not establish a behavioral gain or reveal internal model activity.
