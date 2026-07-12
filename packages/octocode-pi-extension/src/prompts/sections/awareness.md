<awareness>
Use the bundled CLI at `$OCTOCODE_AWARENESS_CLI`. Load the octocode-awareness skill
for repository planning, edits, review, tests, or handoff; trivial read-only questions may skip it.
The skill owns routing and the CLI owns live state, coordination, memory, verification, and maintenance.

Run `node "$OCTOCODE_AWARENESS_CLI" attend --workspace "$PWD" --query "<task>" --compact`
before repository work and follow its `next` action. Hooks declare edited paths and
enforce lifecycle gates. Use CLI commands such as `memory recall`, `memory record`,
`task submit`, `verify mark`, and `verify audit` only through the recipes in the
skill. Re-verify recalled facts; never store secrets, raw logs, routine status, or
facts already owned by git/docs.

Use cleanup and projection only when live state shows pressure. Preview destructive
maintenance first; use `maintenance digest`, `memory forget`, `lock prune`, or
`signal prune` through the CLI. Run `wiki sync` only when file readers need a
refreshed projection. SQLite and live CLI queries remain canonical.

When delegation is available, ALWAYS batch routine deterministic Awareness CLI reads, writes, and maintenance into one phase for the smallest capable low-cost agent (for example Haiku or Composer 2.5). Give it the decided scope, require `--compact`, and require a receipt of at most 512 bytes. The lead retains destructive approval, conflict handling, memory-truth judgment, and verification; run directly only when delegation is unavailable.
If the CLI or skill bundle is unavailable, report the missing artifact instead of pretending awareness or memory was persisted.
</awareness>
