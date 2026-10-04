`generateEtags` (default `true`) controls two things: whether rendered HTML and data responses get an `ETag` header and can be answered with a 304, and whether static files served by the router get an etag. I read the code but did not run it.

**Config definition**
- `packages/next/src/server/config-shared.ts:1824` declares `generateEtags?: boolean`, and line 2213 sets the default to `true`.
- `packages/next/src/server/config-schema.ts:670` validates it as `z.boolean().optional()`.
- `config-shared.ts:2614` copies it into the render options.

**Rendered pages and data responses**
- `packages/next/src/server/base-server.ts:540` and `:577` put `generateEtags` into `renderOpts`. At `:2152` and `:2171` it is read from there and passed to `sendRenderResult`.
- The pages-router handler passes `nextConfig.generateEtags` straight to `sendRenderResult` (`route-modules/pages/pages-handler.ts:739`).
- `next-server.ts:554` and `:563` pass the option through to the `sendRenderResult` call there.
- The effect is in `packages/next/src/server/send-payload.ts:64-71`:
  - Streaming (dynamic) results have `payload = null`, so they never get an ETag, whatever the setting.
  - If `generateEtags` is true and the payload is a string, `generateETag(payload)` computes the tag, and `sendEtagResponse` runs (`:11-33`).
  - `sendEtagResponse` sets the `ETag` header. It then checks the request headers with `fresh()` (`If-None-Match` and similar). On a match it sets status 304, ends the response with no body, and `sendRenderResult` returns early.
  - If the option is false, none of this runs. The response has no `ETag` and is never turned into a 304. It still gets `Content-Type` and `Content-Length`, and the body is sent as normal (`:73-89`).

**Static files**
- `packages/next/src/server/lib/router-server.ts:662-666` calls `serveStatic(..., { etag: config.generateEtags })`. The comment there says it ensures etags are not generated for static files when disabled. The `send`-style static server therefore omits the `ETag` header when the option is false.

I did not read `generateETag` in `lib/etag`, so I haven't checked the hash algorithm or whether the tag is weak.