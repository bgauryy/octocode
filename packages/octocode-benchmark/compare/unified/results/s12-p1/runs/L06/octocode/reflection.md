**1. Helped:** The first `localSearch` for `generateEtags` mapped every use in one call. The batched `localFetch` of `send-payload.ts`, `base-server.ts:2148`, `router-server.ts:655` and `pages-handler.ts:725` showed the deciding branch (`send-payload.ts:66`) and the static-file `etag:` option. The final `localSearch` with context lines on `sendEtagResponse` showed the ETag header and 304 logic.

**2. Did not help:** Nothing errored or repeated. The `localSearch` for `sendEtagResponse` returned the first 33 lines of the file with the match buried in them, which was noisy.

**3. Next time:** I would read the lines I only saw as grep hits. These were `base-server.ts:540/577`, `config-shared.ts:2614` and `next-server.ts:554/563`. I would also read `lib/etag.ts` and the `serveStatic` definition.

**4. Confidence:** High on the core flow, which I read directly:
- `sendRenderResult` generates the ETag only when the option is true and the payload is non-streaming.
- That response can become a 304.
- `router-server.ts:665` passes the option to `serveStatic`.

Medium on the details around it:
- My claims about `next-server.ts` and the render-options copy rest on grep lines only, not the surrounding code.
- I named the `If-None-Match` header from memory, not from the code I read.
- I called `serveStatic` "`send`-style" from the nearby comment, without opening its definition.