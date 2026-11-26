Load when dispatching a diverger or a critic. Other briefs: [gatherer](brief-gatherer.md), [debaters and judge](briefs-debate.md). A brief carries [research questions](research-questions.md), and a gatherer writes a [claim ledger](claim-ledger.md). Who sees what and what you recheck: [workers](workers.md).

# Worker briefs: diverger and critic

## Diverger brief

One pass of [step 3](../SKILL.md#steps): its own lens, persona or constraint, in a fresh context ([why](method-evidence.md#independent-passes)).

```text
Objective: generate options for <decision>; you do not research, rank or choose.
Frame: <decision, constraints, success criteria, what the user ruled out>.
Your pass: <pass name>: <lens question>, thinking as <persona or under constraint>.
Ownership: <run>/passes/<pass>.jsonl; write only this file and spawn no actors.

Work step by step: list candidate options briefly, revise the list until the options differ in mechanism rather than wording, then state each as a testable claim.
Do not read other passes, search results or earlier option lists: they anchor yours.
Write one JSON line per option:
  {"id":"<pass initial><n>","pass":"<pass name>","origin":"yours","option":"<short name>","claim":"<what must be true for it to win>"}
Return: the file path, and the option whose feasibility you doubt most, with why.
```

## Critic brief

Dispatch when the [workers](workers.md) table says to; one critic may cover several directions, and it never gathered the evidence it checks ([independence](judging.md#independence-and-incentives)).

```text
Review <conclusion and verified ledger> before reading others' verdicts.
Try to break it: overclaim, incentives, applicability, independence, baseline, counterevidence, and a claim marked proven that does not meet the proof bar.
Return material issues with claim references, consequence and repair, plus the strongest objection and earned confidence. New evidence needs an opened URL and a verified quote; label unchecked gaps.
```
