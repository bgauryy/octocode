# Runner tool context

Give a runner `RUNNER.md`, its matchup rules, and **only its assigned arm primer** before the
first research call. Keep the primer identical across questions and passes, record the tool
versions (the anchor pins `npx octocode@<ver>`), and never add question-specific advice.

Every arm takes its **leanest legitimate path** (targeted region reads, snippet-bearing
searches, minimal `--json` fields, raw file media) — never a whole-tree or whole-file dump
where a targeted read or search answers.

This fixed setup context is not CLI output and is **excluded from character totals** in both
directions. Only research-time commands (model-out), their returned output (model-in), and
the final answer (model-out) are counted; any later help, catalog, schema, or failed command
is a measured research call.

## Arm primers (inject exactly one)

| Arm | Primer |
|---|---|
| Octocode arm | [`references/primer-octocode.md`](primer-octocode.md) |
| gh + RTK arm | [`references/primer-gh-rtk.md`](primer-gh-rtk.md) |
| gh + Headroom arm | [`references/primer-gh-headroom.md`](primer-gh-headroom.md) |
| gh arm (bare baseline) | [`references/primer-gh.md`](primer-gh.md) |
