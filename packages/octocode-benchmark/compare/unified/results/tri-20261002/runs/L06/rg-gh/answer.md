`generateEtags` (default `true`) controls whether Next.js sets an `ETag` header and answers conditional requests with `304`. It does this in two places: rendered pages and route responses, and static files served from the build output.

All paths below are under `packages/next/src`.

**Config**
- `server/config-shared.ts:1824` declares the option, `:2213` sets the default to `true`, and `server/config-schema.ts:670` validates it as a boolean.

**1. Rendered responses (pages, app pages, data)**
- `server/base-server.ts:540` reads it from `nextConfig`, and `:577` copies it into `renderOpts`.
- At `base-server.ts:2152` it is read back from `renderOpts` and passed to `sendRenderResult` at `:2171`.
- The build templates and handlers pass `nextConfig.generateEtags` to `sendRenderResult` directly. Examples are `build/templates/app-page-runtime.ts:1804` and `server/route-modules/pages/pages-handler.ts:739`.
- `server/send-payload.ts:66` is where it takes effect:
  - **When it applies:** only if `generateEtags && payload !== null`. The payload is `null` when `result.isDynamic` is true (`:63`), so streamed or dynamic results never get an ETag.
  - **ETag:** `generateETag(payload)` (imported from `./lib/etag`) builds the tag, and `sendEtagResponse` (`:11-33`) sets the `ETag` header.
  - **304:** if `fresh(req.headers, { etag })` says the client's cache is current, it sets status `304`, ends the response with no body, and returns. This is the `If-None-Match` handling.
- If the option is `false`, the ETag block is skipped. The response then gets `Content-Type` and `Content-Length` and is sent in full. No ETag is set and no 304 is produced by this path.

**2. Static files**
- `server/lib/router-server.ts:665` passes `etag: config.generateEtags` to `serveStatic`. The comment there says it ensures ETags are not generated for static files when the option is disabled.
- `serveStatic` is built on the `send` module, so I'm inferring that `send` handles the ETag header and 304 for these files. I did not open `serveStatic` or `send` to confirm.

**Uncertainty:** I did not read `generateETag` in `server/lib/etag`, so I can't say which hash algorithm it uses.