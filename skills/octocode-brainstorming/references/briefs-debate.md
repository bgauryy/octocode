Load only when the debate gate holds, to brief independent PRO and CON debaters or a separate judge.

# Debater and judge briefs

## Debater brief

Use only when the [debate gate](judging.md#debate-when-and-how) holds; dispatch independent PRO and CON positions.

```text
Argue <PRO|CON> on "<falsifiable resolution>" using <verified ledger>.
Cite the supporting claim ID; new evidence needs an opened URL and a quote that verify-claims.mjs verifies.
Address the strongest counterpoint, concede what evidence requires, and separate evidence from inference. Say when evidence does not support your side.
Return the case, material concession and evidence that would change the conclusion.
```

## Judge brief

The orchestrator judges by default. Add a separate judge only when independent judgment can change the decision; supply the resolution, arguments and verified ledger.

```text
Judge "<resolution>" from <arguments and verified ledger>.
Count only verified rows whose quote supports the claim in matching conditions. Compare the same criteria and ignore eloquence, length and formatting. Recheck a consequential close verdict in reverse order; if it flips, leave it unresolved.
Return PRO, CON or UNRESOLVED, with deciding evidence, confidence and its limits, and the check most likely to overturn it. Source count alone is not confidence.
```
