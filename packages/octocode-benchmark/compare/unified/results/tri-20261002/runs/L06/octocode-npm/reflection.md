**Helped:** The first `localSearch` (text search for `generateEtags` under `packages/next/src`) found the whole chain in one call: config default, `renderOpts`, the callers, and `send-payload.ts`. The `localGetFileContent` read of `send-payload.ts` lines 30-125 proved the actual behavior (lines 64-71). The second read of lines 1-33 showed `sendEtagResponse`.

**Did not help:** I made no wasted calls. I should have read lines 30-125 and 1-33 as one window, since the split cost an extra call. The search output was noisy: `app-page-runtime.ts` returned ten near-identical rows.

**Overstated:** My answer said I "read the source paths below", but only `send-payload.ts` was read in full. Everything else came from grep snippets of a few lines each: `base-server.ts`, `next-server.ts`, `pages-handler.ts`, `app-page-runtime.ts` and `router-server.ts`. I never opened `router-server.ts` around line 665 or `lib/etag`. I flagged the static-file sender and `generateETag` as unread, but the opening claim was too strong.

**Next time:** I would read `router-server.ts` around line 665 to confirm the static-file claim, and the `lib/etag` implementation. I would use `lspGetSemantics` references to check for other call sites rather than relying on text search.

**Confidence:** High for the rendered-response ETag and 304 behavior, because I read that code directly. Medium for the static-file path, which rests on a grep snippet and its comment.