# ghHistoryResearch (superseded)

> **Deprecated — split into 4 focused tools.** The 52-field `type`-multiplexed
> mega-tool has been replaced. Use the tool that matches your job:
>
> | Was | Now |
> |---|---|
> | `type:"prs"` | **[ghSearchPullRequests](./ghSearchPullRequests.md)** |
> | `type:"issues"` | **[ghSearchIssues](./ghSearchIssues.md)** |
> | `type:"commits"` | **[ghSearchCommits](./ghSearchCommits.md)** |
> | `type:"releases"` | **[ghListReleases](./ghListReleases.md)** (opt-in: `ENABLE_RELEASES=1`) |
>
> Each new tool exposes only its own fields (no `type` discriminator, no
> mode-irrelevant params). `ghHistoryResearch` remains registered during the
> migration window but should not be used in new flows.
