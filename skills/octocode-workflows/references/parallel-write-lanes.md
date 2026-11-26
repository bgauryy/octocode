Load when several actors change one repository at once: path ownership, shared builds and generated artifacts, a contract-breaking change, or redirecting a lane.

# Parallel write lanes

Partition the writing before launch: no actor edits until every changed path has one owner in a Task contract.

| Concern | Rule |
|---|---|
| Ownership | Each Task contract names the paths the actor owns and the owner of each neighboring path. A fix needed outside the actor's paths goes back as a request in its return, never as an edit. Where two lanes must share a file, each makes small exact edits and re-reads the file on conflict. |
| Shared mutable resources | Generated artifacts get one locked regenerate command. The orchestrator owns the build and the lockfile. Lanes share one build cache, never a private target or cache directory each. The [machine budget](machine-budget.md) decides when a heavy job may start. |
| Contract-breaking change | The orchestrator lands the upstream contract change and regenerates before lanes adapt to it. Otherwise one lane owns the change and every consumer it breaks. |
| Half-finished neighbors | A compile or test failure in another lane's files is re-run before it is blamed. If it persists, report it with the file and line. |
| Redirect | When evidence contradicts a lane's direction, send it the measured evidence, the new target, and what to remove. Stopping early costs less than merging the wrong design. |
| User hold | "Plan first" or a design question pauses every edit the plan would change. Lanes continue only the work the plan leaves unchanged. |
| Foreign effects | When something outside the lanes changes shared state (index staging, background jobs), report it. Never stash, reset, or overwrite to hide it. |

Size a lane by its files and checks, not by item count. Once its return is in, re-read the deciding anchors before accepting, as the [orchestrator merge](composition.md#orchestrator-and-actor-contract) requires.
