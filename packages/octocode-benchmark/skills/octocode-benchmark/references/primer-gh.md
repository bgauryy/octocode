# gh arm primer

Inject as the `gh` runner's only primer. This is the **uncompressed baseline**: plain GitHub
CLI with no transport wrapper and no compression layer. Every research call is `gh <args>`
run through the arm's instrumentation wrapper. Allowed read-only forms:

```bash
gh search code|repos|prs|issues|commits ...
gh repo view OWNER/REPO ...
gh pr view|diff NUMBER --repo OWNER/REPO ...
gh issue view NUMBER --repo OWNER/REPO ...
gh api 'repos/OWNER/REPO/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'
gh api 'repos/OWNER/REPO/git/trees/SHA?recursive=1'
```

Read-only only — no mutation verbs (no `create`, `edit`, `merge`, `close`, `comment`, or a
non-GET `api` call). Prefer snippet-bearing `gh search code` hits that already answer the
question (avoid a full-file fetch), request raw file media rather than base64 `--jq .content`
(~1.33× larger), and keep `--json` field lists minimal. `gh` adds no repositories or evidence
beyond what these calls return, and nothing is compressed — this is the raw context cost the
`rtk` and `headroom` matchups compress.
