`generateEtags` (default `true`) controls whether Next.js's Node server adds an `ETag` header to rendered responses and answers conditional requests with a 304. It also controls ETags on static files served by the router server. I did not trace other paths, such as app-route handlers, image optimization, or the minimal-mode and edge paths. The search hit a cap and was marked partial.

**Config plumbing**
- The option is declared at `packages/next/src/server/config-shared.ts:1824` and validated as an optional boolean at `config-schema.ts:670`. The default is `true` at `config-shared.ts:2213`.
- `config-shared.ts:2614` copies it into the server-side config subset.
- `BaseServer` reads it from `nextConfig` (`base-server.ts:540`) and stores it in `renderOpts` (`base-server.ts:577`).

**Rendered pages and data responses**
- `base-server.ts:2152` reads `generateEtags` from `renderOpts`. It passes the value to `sendRenderResult` at `base-server.ts:2169-2174`, via the `NextNodeServer` wrapper at `next-server.ts:554-566`.
- The pages route module passes `nextConfig.generateEtags` directly at `route-modules/pages/pages-handler.ts:739`.
- In `send-payload.ts:64-71`, the payload is only buffered when the result is not dynamic (`result.isDynamic ? null : result.toUnchunkedString()`). If `generateEtags && payload !== null`, it computes `generateETag(payload)` and calls `sendEtagResponse`.
- `sendEtagResponse` (`send-payload.ts:11-33`) sets the `ETag` header at line 23. If the request's conditional headers match, via `fresh(req.headers, { etag })`, it sets status 304, ends the response with no body, and returns `true`. The caller then returns early.
- When the option is `false`, no `ETag` is set and no 304 is produced on this path. The response is sent normally: `Content-Length`, then `res.end(payload)` or a stream.
- Dynamic or streamed results never get an ETag on this path, whatever the setting, because `payload` is `null`.

**Static files**
- `lib/router-server.ts:662-666` passes `etag: config.generateEtags` to `serveStatic`, with the comment "Ensures that etags are not generated for static files when disabled." So the option also turns the `send` library's ETag generation on or off for those files.