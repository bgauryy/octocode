# File Awareness

Use `work.show` with an exact file when concurrent activity could alter the next action. Declared presence is advisory and does not prove authorship, completeness, or conflict.

If work overlaps but remains mergeable, coordinate with `message.send` and continue with explicit ownership. For non-mergeable overlap, stop and use `work.protect`. Never infer safety from expiry; re-read current work state before editing.

Use Octocode search and LSP for repository evidence. Awareness tracks coordination state; it does not replace source inspection.
