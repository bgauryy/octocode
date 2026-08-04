# Octocode CLI vs `ast-grep`

Ten local research questions in [`questions/`](questions/), run on a pinned React checkout.

| Arm | Allowed surface |
|---|---|
| A | `ast-grep` CLI |
| B | Octocode local tools through `npx octocode tools …` |

Both runners use the same read-only corpus. Neither gets GitHub, browser, peer, or grader-reference access. Confirm both CLIs plus the corpus commit before running.

## Corpus (pinned)

Every `$CORPUS/...` path and every count in `questions/` is defined against **one exact commit** of `facebook/react`. Clone it once, read-only, and export `$CORPUS`:

```bash
git clone https://github.com/facebook/react /tmp/react-corpus
git -C /tmp/react-corpus checkout 7dfc7ccd12d0294debc69b9b9b4e9dd1fd42e08a
export CORPUS=/tmp/react-corpus
git -C /tmp/react-corpus rev-parse HEAD   # must print 7dfc7ccd12d0294debc69b9b9b4e9dd1fd42e08a
```

- **Pinned SHA:** `7dfc7ccd12d0294debc69b9b9b4e9dd1fd42e08a` (`main`, 2026-08-03).
- Counts are only reproducible/gradeable at this SHA. If you re-pin, re-run every count question and update this line.
- Both arms read the same clean checkout; do not modify it during a run.
