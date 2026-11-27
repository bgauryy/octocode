Load when choosing the test level of a case or writing the oracle that decides pass or fail.

# Levels and oracles

## Pick the level

Apply the plan loop's cheapest-level rule with this table:

| Level | Observes | Use when |
|---|---|---|
| Unit | One function or module with its inputs | Logic, boundaries and error mapping inside one unit |
| Integration | Real collaborators: file system, parser, database, process | The risk is in the seam between units or in configuration |
| End-to-end | The built artifact through its public entry (CLI, HTTP, UI) | The claim is user-visible behavior, packaging or wiring; a case that runs the real program on fixture files is end-to-end |
| Manual or exploratory | A person or agent judging an open question | No automatic oracle exists; give a time box and what to look for |

Use an existing harness or fixture when one reaches the behavior. A new fixture is part of the case's **Given**: say where it goes and what it contains.

## Write the oracle

Take the first form the behavior allows:

1. Exact value: exit code, status code, an exact string or JSON field value.
2. Pattern or property: output matches a regex, a list is sorted, a count equals the input count, a round trip returns the input.
3. Differential: the new path gives the same result as the old version, a reference implementation or a second tool.
4. Bound: a timing or size bound with unit, baseline and number of runs.
5. Human judgment: only for manual cases, with the question to answer.

When no expected value can be computed (the oracle problem: ranking, numeric models, generated output), use a property or metamorphic relation: a change to the input with a known effect on the output, such as reordering input leaves a sorted result unchanged.

Apply the [plan loop’s oracle rule (step 4)](../SKILL.md#plan-loop): a convenient assertion does not establish expected behavior.
