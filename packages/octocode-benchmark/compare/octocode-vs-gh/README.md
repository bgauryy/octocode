# Octocode CLI vs `gh`

Seventeen GitHub research questions in the shared set [`../github-questions/`](../github-questions/) — one canonical copy, used by all three GitHub matchups.

| Arm | Allowed surface |
|---|---|
| A | Read-only `gh` repository, code, content, tree, PR, issue, and commit operations |
| B | Matching GitHub research through `npx octocode tools …` |

Both runners receive the same question and budget. Neither gets browser, local-code, peer, or grader-reference access.

The same shared set is used by `octocode-vs-gh-rtk` and `octocode-vs-gh-headroom`.

## How to run Arm A (`gh`)

Confirm `gh --version` and that `gh auth status` is authenticated before running.
Allowed families (read-only — no mutation verbs): `search
{code,repos,prs,issues,commits}`, `repo view`, `pr view|diff`, `issue view`, and
`api` limited to GET on `/contents` or `/git/trees`.

```bash
gh search code --repo vercel/next.js "getRouteRegex" --limit 20
gh api 'repos/vercel/next.js/git/trees/canary?recursive=1'
gh api 'repos/vercel/next.js/contents/PATH?ref=canary' \
      -H "Accept: application/vnd.github.raw"     # raw, not --jq .content base64 (~1.33× larger)
```

Footprint: prefer a snippet-bearing `gh search code` hit when it already answers
the question (avoids a full-file fetch); keep `--json` field lists minimal.
Record the characters each call pulls into context — that is Arm A's chars-in
for `SCORING.md`. This is the uncompressed baseline the `rtk` and `headroom`
matchups compress.

## Arm B (Octocode)

```bash
npx octocode tools <the-question>
```

Record chars in/out per question, same as Arm A.
