**Short answer:** `generateEtags` (default `true`) controls two things. For rendered pages, it decides whether `sendRenderResult` computes an `ETag` header and can answer a conditional request with a 304. For static files served by the router server, it is passed to `send` as its `etag` option.

**Config:**
- The option is declared in `packages/next/src/server/config-shared.ts:1824`. The default is `true` at `config-shared.ts:2213`.
- The zod schema accepts it as an optional boolean at `config-schema.ts:670`.
- It is copied into the runtime server config at `config-shared.ts:2614`.

**Rendered pages and data responses:**
1. `base-server.ts:540` reads `generateEtags` from `nextConfig`, and `base-server.ts:577` puts it into `renderOpts`.
2. `base-server.ts:2152` reads it from `renderOpts` and passes it to `sendRenderResult` at `base-server.ts:2169-2174`.
3. `next-server.ts:554-566` forwards it to `sendRenderResult` in `send-payload.ts`.
4. The pages route module bypasses `renderOpts`. `pages-handler.ts:739` passes `nextConfig.generateEtags` directly.
5. In `send-payload.ts:64-71`, the payload is `null` for dynamic or streamed results. Otherwise it is the full string. Then:
   ```ts
   if (generateEtags && payload !== null) {
     const etag = generateETag(payload)
     if (sendEtagResponse(req, res, etag)) return
   }
   ```
6. `sendEtagResponse` (`send-payload.ts:11-33`) sets the `ETag` response header. It then runs `fresh(req.headers, { etag })`. If the client's `If-None-Match` matches, it sets status 304, ends the response with no body, and returns `true`.

With the option set to `false`, no `ETag` is generated for these responses and no 304 is produced from the payload. The response then continues to set `Content-Type` and `Content-Length`, and sends the body. Streamed or dynamic results never get an ETag, whatever the setting.

**Static files:**
- `router-server.ts:662-666` calls `serveStatic(...)` with `etag: config.generateEtags`. The comment there says it ensures etags are not generated for static files when disabled.

**Not verified:** I did not read `generateETag` in `lib/etag` or the `serveStatic` implementation. I also did not check whether other paths, such as app-route handlers, use ETags independently of this option.