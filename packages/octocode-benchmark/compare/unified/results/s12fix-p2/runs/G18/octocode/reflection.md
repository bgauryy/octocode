1. **Helped:**
- `artifactSearch` (npm, undici 6.21.0) was the fastest step. One call returned `engines: ">=18.17"`, the publish date and a release-source SHA hint.
- `ghGetHistoryItem` on `v6.21.0` showed the tag resolves to the same SHA, `61ec3531…`.
- `ghGetFileContent` on `lib/dispatcher/client.js` at `v6.21.0`, with `matchString`, returned the exact default lines (229, 238, 239) without reading the whole file.

2. **Did not help:**
- The `ghSearchCode` call returned empty. It searches the default-branch index, not the 6.21.0 tag, so it could not have answered a version-specific question. I should have skipped it.
- The registry hint was marked `unverified`, and no tool exposed the package's `gitHead` or the actual `package.json` at the tag. The SHA match therefore rests on the tag and the hint agreeing, not on registry provenance.
- The `matchString` read omitted large line ranges. That was fine here, but I did not see the keep-alive header-handling code.

3. **Next time:** run `artifactSearch`, then `ghGetHistoryItem` on the tag, then a single `matchString` read of the file at the tag, skipping the code search. I would also read `package.json` at the tag to confirm `engines` directly.

4. **Confidence:** high on the engines range, the commit and the three defaults, since each has a direct source line or metadata. It is slightly lower on "published from" for the reason above. The keep-alive header nuance is stated from memory, and I flagged it as unread.