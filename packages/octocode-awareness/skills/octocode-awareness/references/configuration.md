# Awareness configuration

Missing workspace configuration uses package defaults. Create or change configuration only when you request different storage or lifecycle behavior.

Workspace policy lives at `.octocode/awareness.json` and owns two decisions:

When validating policy fields, read the [configuration schema](awareness-config.schema.json) for the accepted structure.

- `storage.repository` and `storage.memory` select the database scope.
- `hooks.profile` selects lifecycle breadth, while `hooks.owners` selects `shell` or `native` per host.

All participants must reuse the same resolved database and normalized workspace identity. A scope change does not merge existing stores. Never hand-edit SQLite rows.

Pi claims native ownership and does not install shell hooks. Shell-host installation requires a scoped dry-run preview, existing authorization, and a strict post-install check. Configuration preference alone is not authorization to mutate host settings.
