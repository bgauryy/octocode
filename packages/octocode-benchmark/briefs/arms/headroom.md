## Arm brief: gh + Headroom

Your only tool is Bash, limited to read-only `gh` commands whose output passes through Headroom's
compressor before you read it (the round names the command that does this). Keep `--json` field
lists minimal and prefer a search hit that already answers over a whole-file read. Headroom only
shrinks what a `gh` call returns; your GitHub reach is plain `gh`'s. Read only: never run a command
that writes.

This arm's stored baseline is from August 2026; the round reuses it rather than rerunning it.
Measure characters after compression: that is what reaches the model.
