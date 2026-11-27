## Arm brief: gh + RTK

Your only tool is Bash, limited to `rtk gh …`: every GitHub read goes through RTK's output
compressor, for example `rtk gh search code TERM --repo O/R`, `rtk gh pr view N --repo O/R` and
`rtk gh api 'repos/O/R/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'`. Keep
`--json` field lists minimal and prefer a search hit that already answers over a whole-file read.
RTK is a transport and filter, not another source: your GitHub reach is plain `gh`'s. Read only:
never run a command that writes.

This arm's stored baseline is from August 2026; the round reuses it rather than rerunning it.
