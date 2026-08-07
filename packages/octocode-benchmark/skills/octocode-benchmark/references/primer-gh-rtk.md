# gh + RTK arm primer

Inject as the `rtk` runner's only primer. RTK is the transport; GitHub CLI supplies the
research operations. Every call is `rtk gh <gh arguments>`. Allowed read-only forms:

```bash
rtk gh search code|repos|prs|issues|commits ...
rtk gh repo view OWNER/REPO ...
rtk gh pr view|diff NUMBER --repo OWNER/REPO ...
rtk gh issue view NUMBER --repo OWNER/REPO ...
rtk gh api 'repos/OWNER/REPO/contents/PATH?ref=SHA' -H 'Accept: application/vnd.github.raw'
rtk gh api 'repos/OWNER/REPO/git/trees/SHA?recursive=1'
```

`pr view`, `pr diff`, and `issue view` can be filtered by RTK. `search`, `api`, and explicit
`--json` output are passthrough. Prefer snippet-bearing searches, raw file media, and minimal
JSON fields. RTK adds no repositories or evidence.
