## Arm brief: gh, lean

Your only tool is Bash, limited to read-only `gh` plus the text filters `grep`, `sed`,
`head`, `tail`, `wc`, `sort`, `uniq`, `cut` and `jq` on gh's output. Make every byte you
read back count:

- **Project, don't dump.** `--json` with only the fields you need, `--jq` to cut them down
  further, `--limit` to cap rows. `gh api … --jq '<expr>'` for API calls.
- **Select at the server when supported.** `gh repo view O/R --json nameWithOwner,description,stargazerCount`
  and `gh api graphql -f query='query { … }'` select GraphQL fields. REST `--jq` trims the
  response locally; it does not reduce the fetched payload. Keep GraphQL read-only.
- **Search with snippets.** `gh search code TERM --repo O/R --json path,textMatches --jq …`;
  a matching fragment that answers ends the question.
- **Read regions, not files.** When installed, use the preview reader:
  `gh repo read-file PATH --repo O/R --ref SHA | sed -n '120,170p'`. Use its JSON view for
  `path,gitSHA,size`; `gitSHA` identifies the blob, not its containing commit. JSON `content`
  is base64; use streamed raw content for regions.
  The equivalent raw API form is:
  `gh api 'repos/O/R/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw' | grep -n 'symbol'`
  and `… | sed -n '120,170p'`. Never base64 `--jq .content`.
- **Trees by path.** `gh api 'repos/O/R/git/trees/SHA:sub/dir' --jq '.tree[].path'`; add
  `?recursive=1` only on a narrow subtree you truly need.
  When installed, `gh repo read-dir sub/dir --repo O/R --ref SHA --json path,type,size`
  lists one directory through the preview GraphQL reader. Check `truncated` on recursive REST trees.
- **History.** `gh search prs|issues|commits … --json …`, `gh pr view N --repo O/R --json files,title`,
  `gh pr diff N --repo O/R | grep -n …`, `gh api repos/O/R/compare/A...B --jq …`.
- **Finish the selected pages.** `gh api ENDPOINT --paginate --jq '…'` filters each page.
  For an aggregate, use `gh api ENDPOINT --paginate --slurp | jq '…'`; installed gh rejects
  combining `--slurp` with `--jq` or `--template`. GraphQL pagination needs `$endCursor` and
  `pageInfo { hasNextPage endCursor }`. A row limit or selected field is not proof of completeness.
- **Match freshness.** `gh api ENDPOINT --cache 5m` is an optional TTL-cached comparison tier,
  only when five-minute staleness is permitted by the experiment. Omit it for current reads;
  report first and repeat calls separately. Use the installed command's help for supported flags.

One command per call: `gh …` alone or piped into the filters above. Shell variables,
`$(…)`, `cd`, `;`/`&&` chains and temp files are not allowed (a call that uses them counts as
failed), so put a SHA you found straight into the next command. Read only: never run a
command that writes, including local `read-file --output`/`--clobber`; stream file content to
the permitted filters.
