Load when briefing the small-model interviewee of a [cold interview](interview.md).

# Interviewee brief

Spawn a fresh small-model agent per group per round, never one that saw an earlier round:

```text
You are interviewed on a protocol: the instructions and tool schemas in PACK.
Read the relevant supplied protocol before answering. Cite the deciding text or missing rule.
Edit nothing except files in OUT.

Part 1 (for the instructions, then each tool in group GROUP):
1. What did you understand: job, when to use, when not, inputs, outputs, next step?
2. What is unclear or ambiguous? Quote it.
3. Which assumptions could break?
4. Where is the protocol incomplete?
5. Which edge cases might you fail on?
6. When would you ask the user for clarification?
7. How would you measure success or failure of a call and of an answer?

Part 2, Phase A (cold): for each scenario in SCENARIOS, write the first calls you would make
and your predicted output to OUT/GROUP-calls.jsonl, one single-line JSON object per line,
concrete values only (describe chained steps in "expect"). Write the file before running anything.
Part 2, Phase B: run each call with RUNNER (read-only), then chained calls until answered or
blocked. Record the outcome, where real output differed from your prediction, what you changed,
which protocol text would have prevented the mistake, and the cited answer.

Report actionable gaps with the deciding protocol text, observed effect and proposed repair;
compare predicted and actual calls. Include blockers and what remains unverified.
```

Cold-call line:

```json
{"scenario":"S2","step":1,"tool":"<tool>","input":{"queries":[{}]},"expect":"<predicted key fields and next action>"}
```
