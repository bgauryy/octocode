`generateEtags` (default `true`) turns ETag generation on or off in two places. One is rendered HTML and RSC responses, which go through `sendRenderResult`. The other is static files served by the router server, where it becomes the `etag` option of `serveStatic`. When it's `false`, Next.js sends no `ETag` header and never returns a 304 for these responses.

**Config definition**
- `packages/next/src/server/config-shared.ts:1824` declares `generateEtags?: boolean`.
- `config-shared.ts:2213` sets the default to `true`.
- `packages/next/src/server/config-schema.ts:670` validates it as `z.boolean().optional()`.

**Rendered pages and RSC**
- `base-server.ts:540` and `:577` read `generateEtags` from `nextConfig` and put it into `renderOpts`.
- `base-server.ts:2152` and `:2171` take it back out of `renderOpts` and pass it to `this.sendRenderResult(...)`.
- `next-server.ts:554-563` forwards it to the standalone `sendRenderResult`.
- The build-time handlers pass `nextConfig.generateEtags` directly, for example:
  - `build/templates/app-page-runtime.ts:1804`, `:1822`, `:1925`, `:2154`
  - `route-modules/pages/pages-handler.ts:739`
- The effect is in `send-payload.ts:66-71`:
  - The check is `if (generateEtags && payload !== null)`. The payload is `null` when `result.isDynamic` is true (`send-payload.ts:64`), so only non-streaming, fully buffered results get an ETag.
  - It computes `generateETag(payload)` and calls `sendEtagResponse`.
  - `sendEtagResponse` (`send-payload.ts:10-30`) sets the `ETag` header. If `fresh(req.headers, { etag })` says the client's conditional headers match, it sets status 304, ends the response with no body, and `sendRenderResult` returns early.
  - Otherwise it continues and sends the normal 200 response.
  - With `generateEtags` false, this whole block is skipped.

**Static files**
- `lib/router-server.ts:662-665` calls `serveStatic(..., { etag: config.generateEtags })`. The comment there reads "Ensures that etags are not generated for static files when disabled."

I didn't read `serveStatic` or the `send` library to confirm how they handle `etag: false`. I also didn't check whether other code paths, such as image optimization, set ETags independently of this option.