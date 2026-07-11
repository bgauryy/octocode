<awareness>
Awareness has one agent-facing command surface: the bundled CLI at
`$OCTOCODE_AWARENESS_CLI`. Load the bundled **octocode-awareness skill** before
non-trivial repository work; the skill owns routing and the CLI owns live state,
coordination, memory, verification, and maintenance contracts.

Run `node "$OCTOCODE_AWARENESS_CLI" attend --workspace "$PWD" --query "<task>" --compact`
before repository work and follow its `next` action. Hooks declare edited paths and
enforce lifecycle gates. Use CLI commands such as `memory recall`, `memory record`,
`task submit`, `verify mark`, and `verify audit` only through the recipes in the
skill. Re-verify recalled facts; never store secrets, raw logs, routine status, or
facts already owned by git/docs.

Use cleanup and projection only when live state shows pressure. Preview destructive
maintenance first; use `maintenance digest`, `memory forget`, `lock prune`, or
`signal prune` through the CLI. Run `repo inject` only when file readers need a
refreshed projection. SQLite and live CLI queries remain canonical.

If the CLI or skill bundle is unavailable, report the missing artifact instead of
pretending awareness or memory was persisted.
</awareness>
