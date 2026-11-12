# Rank the damage and write the roast

Load after findings survive their countercheck: severity levels, deduplication, large targets and delegation, then how to build each bit.

## Rank the damage

Set severity by demonstrated impact, reachability and affected scope, not by how funny or suspicious the code looks. A swallowed error or blocking call is not automatically high severity.

| Severity | Demonstrated consequence |
|---|---|
| Critical | Reachable authentication bypass, exploitable credential exposure or catastrophic data loss |
| High | Major incorrect behavior, substantial data loss or unavailable core functionality |
| Medium | Bounded correctness failure, misleading results or measurable degradation |
| Low | Limited user or maintenance impact with a concrete consequence |

Deduplicate symptoms under their root cause. Keep unresolved leads separate with the next deciding check; do not inflate the finding count or force a defect into every category.

For a large target, prioritize by risk and state coverage limits. Delegate only with user or governing authorization, assigning independent domains and exact paths. Collect findings, checked and cleared scope from each reviewer. Deduplicate, re-verify consequential findings and rank once.

## Write the roast

Build each bit from the verified contradiction: **setup → reversal → concrete consequence**. Use a short analogy, escalation or callback only when it sharpens that contradiction. Keep the punchline beside its finding; do not bury evidence under a monologue.

- Replace generic insults (“spaghetti,” “garbage,” “junior code”) with the exact behavior that deserves the joke.
- For aggressive stand-up, shorten the setup, sharpen the reversal and use forceful language about the implementation. Keep severity unchanged.
- Attack decisions and behavior; do not invent motives, competence claims or personal insults about the author.
- If the joke no longer works after removing the alleged defect, verify the defect again. If the evidence disappears after removing the joke, discard the finding.
- If nothing consequential survives verification, say so. Do not manufacture material to satisfy the requested tone.
