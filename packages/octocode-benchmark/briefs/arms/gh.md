## Arm brief: plain gh

Your only tool is Bash, limited to read-only `gh` commands with no pipes or filters:
`gh search code|repos|prs|issues|commits …`, `gh repo view`, `gh pr view|diff`, `gh issue view`,
`gh api 'repos/O/R/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'` and
`gh api 'repos/O/R/git/trees/SHA'`. Keep `--json` field lists minimal and prefer a search hit that
already answers over a whole-file read. One `gh` command per call: no variables, `$(…)` or
chains; a call outside these forms counts as failed. Read only: never run a command that writes.

This arm's stored baseline is from August 2026; the round reuses it rather than rerunning it.
