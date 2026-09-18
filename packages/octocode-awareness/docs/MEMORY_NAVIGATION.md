# Efficient routine use

Awareness is optimized around one bounded orientation and targeted follow-up operations. Do not load a catalog, workboard, inbox, Memory search, and History timeline when one decision packet answers the next question.

## Start once

Call `context.orient` or reuse a host briefing. Supply `if_revision` on a later orientation only for the same database, workspace, actor, and filter scope. An unchanged result returns the revision without repeating the packet.

Refresh orientation when a peer, Message, Work transition, verification receipt, or recovery state can change the next action. Do not poll on a timer.

## Drill down by owner

| Need | Targeted operation |
|---|---|
| Existing objective or attempt | `work.list` or `work.show` |
| Available dependent task | `work.list` with the required kind |
| Verification debt | `work.verify` with `action: audit` |
| Expected peer response | `message.list` |
| Reusable lesson | `memory.recall` |
| Recoverable file version | `history.timeline`, then `history.read` |

Use returned IDs instead of repeating broad searches. Request Message bodies only when acting on those Messages. Use the narrowest workspace, file, kind, query, and limit supported by the operation schema.

## Preserve partial state

Bounded results must distinguish complete, partial, and terminal-limit outcomes. Preserve:

- `partial` and `partialReasons`.
- Omitted counts and cursors.
- `terminalLimit` diagnostics.
- Executable `next` or retry calls.

Execute continuations with the same trusted bindings. If a continuation repeats a page or loses its cursor, stop and report the contract failure instead of guessing that the data is complete.

## Memory discipline

Memory is for a verified lesson that can change a later decision. It is not a completion log or a second inbox. Recall with the narrowest scope and revalidate the referenced source when a digest, validity condition, or workspace changed.

LocalGit operation IDs can ground a Memory, but History owns byte retrieval. A compact pointer is usually enough until exact bytes affect the decision.

## Measure token claims

Output bytes and prompt tokens are different measures. A smaller response does not by itself prove a cheaper or more reliable workflow. Measure the whole verified path, including discovery, partial-page continuation, retries, communication, and repair.

For operation ownership, see [How Awareness works](HOW_IT_WORKS.md). For reusable evidence rules, see the skill's [Memory reference](../skills/octocode-awareness/references/memory-recall.md).
