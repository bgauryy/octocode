`generateEtags` (default `true`) controls two things: whether Next.js adds an `ETag` header, and whether it can answer a conditional request with a 304. It applies to rendered pages and to static files served by the router server.

**Config definition**
- It is typed as `generateEtags?: boolean` at `packages/next/src/server/config-shared.ts:1824`.
- It defaults to `true` at `config-shared.ts:2213`.
- It is validated as an optional boolean in `packages/next/src/server/config-schema.ts:670`.

**Rendered pages and data responses**
1. `base-server.ts:540` and `:577` read it from `nextConfig` into the render options.
2. On the response path, `base-server.ts:2152` reads it from `renderOpts`. It is passed to `sendRenderResult` at `base-server.ts:2171`.
3. Compiled pages routes pass it straight from `nextConfig`. See `route-modules/pages/pages-handler.ts:739`. App routes do the same in `build/templates/app-page-runtime.ts`, for example at `:1804` and `:1822`.
4. The actual effect is in `sendRenderResult` at `send-payload.ts:66-71`:
   - The condition is `generateEtags && payload !== null`. `payload` is null for dynamic/streamed results (`result.isDynamic`, line 63). So streamed responses never get an ETag, whatever the setting.
   - When it applies, it computes `generateETag(payload)` and calls `sendEtagResponse`.
   - `sendEtagResponse` (`send-payload.ts:11-33`) sets the `ETag` header. If `fresh(req.headers, { etag })` matches the request's `If-None-Match`, it sends a 304 with an empty body and returns early.
   - With `generateEtags: false`, none of this runs. There is no `ETag` header and no 304 for rendered output. The response is a normal 200 with `Content-Length` and the body.

**Static files**
- `router-server.ts:662-665` passes `etag: config.generateEtags` to `serveStatic`. The inline comment there says this stops etags being generated for static files when disabled.
- I did not open `serveStatic` itself. I'm inferring from the option name that it is the `send` module's `etag` option, so `false` would omit the `ETag` header on those files.

**Not verified**
- I did not read the `generateETag` implementation (the hashing).
- I did not check the `next-server.ts:554-563` option plumbing in detail.