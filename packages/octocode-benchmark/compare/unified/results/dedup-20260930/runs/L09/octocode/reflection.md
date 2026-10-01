**1. Helped:** The first `localSearch` for `StaleNaN` in `go/scrape` found `updateStaleMarkers` and `reportStale` in one call. The regex `localSearch` for `updateStaleMarkers|…|endOfRunStaleness|…` then gave the whole call chain at once. The targeted `localFetch` line ranges (1660-1712, 1780-1840, 1120-1200, 1405-1425) were cheap and decisive. Batching several fetches per call saved rounds.

**2. Did not help:**
- My first `localSearch` failed because `exclude` needs an array and I wrote it as a string. I retried immediately.
- In one `localFetch` batch, a row with an empty `reasoning` was rejected on a pattern check. That cost a round.
- `matchString: "reportStale("` returned an omitted-lines gap, so I had to issue another fetch.
- Paginated search results (26 and 20 matches, pages 2-3) were never read, so the coverage I can claim is incomplete.
- The final `localFetch` of 1560-1610 ended mid-comment ("A failed scrape is the same as an empty scrape,"), so I never saw the failed-scrape handling. I stated this as unverified in the answer.

**3. Next time:** Fetch the exact range after the comment at `scrape.go:1610`. Read page 2 of the search results. Check the `target.go` `IsStaleNaN` uses. Make `reasoning` fields non-empty and `exclude` an array from the start.

**4. Confidence:** High for the two main mechanisms, because I read the code at the cited lines. Medium for the failed-scrape claim, and I flagged it as unverified.