**Helped:**
- `artifactSearch` (npm, undici 6.21.0) was the fastest step. It returned `engines >=18.17` and the release-source SHA in one call.
- `ghGetHistoryItem` on `v6.21.0` confirmed that the tag resolves to the same SHA.
- `ghGetFileContent` with `matchString` on `lib/dispatcher/client.js` returned the exact default lines (229, 238, 239) with `commitSha` attached.

**Did not help:**
- `ghSearchCode` for the three timeout keywords in `lib/dispatcher/client.js` returned empty. That tool searches the default branch's index, not the tag, so it was wasted. I ran it in parallel with the commit lookup, so it cost little.
- The `matchString` output omitted line ranges, such as 97-115. That didn't matter here, but it would for questions needing those lines.
- The npm hint labelled the source SHA "unverified". My tag check only shows that the tag and the hint agree. It is not independent proof of what npm published from.

**Next time:**
- Skip the code search. Go straight to `ghGetFileContent` with `matchString` at the tag.
- Read `package.json` and `client.js` in one batched call.
- Check the repo's release or provenance metadata, if it can be reached, to verify the publish commit.

**Confidence:**
- **High:** engines and defaults, because both were read directly at the pinned commit.
- **Medium-high:** that 61ec353 is the published commit. It rests on the tag and the npm hint agreeing, not on provenance.