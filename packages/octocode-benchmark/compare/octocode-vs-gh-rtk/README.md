# Octocode CLI vs `gh` + `rtk`

Seventeen GitHub research questions in the shared set [`../github-questions/`](../github-questions/) — one canonical copy, used by all three GitHub matchups.

| Arm | Allowed surface |
|---|---|
| A | Read-only `gh` operations invoked through `rtk gh` |
| B | Matching GitHub research through `npx octocode tools …` |

RTK is a transport/filter layer, not an additional research source.

These live once in [`../github-questions/`](../github-questions/); edit them there and every GitHub matchup sees the change.

## Arm A (gh + rtk) — run the runner this leanest legal way

Give arm A its fair minimum footprint. Verified against rtk's docs (`rtk-ai/rtk`, "filters and compresses command outputs") and empirically on 2026-08-04:

- **File content — use the raw media type, never base64.** gh has no region-targeted read, so fetch the whole file with `Accept: application/vnd.github.raw`, not `--jq .content` (base64 is ~1.33–1.36× larger; measured 102,534→75,638 bytes). Still within `gh-rtk-readonly` (family `api`, `/contents` scope, GET):
  `rtk gh api repos/OWNER/REPO/contents/PATH?ref=SHA -H "Accept: application/vnd.github.raw"`
- **PR/issue detail & diffs — let rtk filter.** `rtk gh pr view N`, `rtk gh pr diff N`, `rtk gh issue view N` **without `--json`** get rtk's compact output (measured `rtk gh pr diff` 1811→1534, ~15% smaller). Add `--json <fields>` only when an exact structured field is required — `--json` forces passthrough (rtk adds nothing).
- **`search code` / `api` are passthrough.** rtk does not compress these (measured `gh search code` == `rtk gh search code`); expect no rtk savings and keep queries tight.
- **Prefer snippet-bearing `gh search code`** when its hit already answers the question, to avoid a full-file fetch entirely.

Note per call whether rtk **filtered** or **passed through** (`--json`, `search`, and `api` are passthrough — rtk adds nothing). This guidance changes footprint, not the read-only policy.

Allowed arm-A families: `search {code,repos,prs,issues,commits}`, `repo view`, `pr view|diff`, `issue view`, and `api` limited to `/contents` or `/git/trees` (GET only). No mutation verbs.

Completed runs of this matchup are in [`../../results/`](../../results/) (see the index there for the latest).
