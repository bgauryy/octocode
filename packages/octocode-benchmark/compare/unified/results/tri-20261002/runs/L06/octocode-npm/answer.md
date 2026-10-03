`generateEtags` (default `true`) controls two things. It decides whether Next.js adds an `ETag` header to rendered HTML and data responses, and whether it can answer a conditional request with a 304. It also sets the `etag` option on static file serving. I read the source paths below but did not run anything.

All paths are under `packages/next/src/`.

**Config and defaults**
- `server/config-shared.ts:1824` declares the option, `server/config-shared.ts:2213` sets the default `generateEtags: true`, and `server/config-schema.ts:670` validates it as `z.boolean().optional()`.
- `server/config-shared.ts:2614` copies it into the config object for the server.

**Rendered responses (pages, app routes, data)**
- `server/base-server.ts:540` reads `generateEtags` from `nextConfig`, and `server/base-server.ts:577` puts it into `renderOpts`.
- At `server/base-server.ts:2152` and `server/base-server.ts:2171`, the response sender reads it from `renderOpts` and passes it to `sendRenderResult`. `server/next-server.ts:554-563` forwards it to `sendRenderResult` in `send-payload.ts`.
- The pages handler (`server/route-modules/pages/pages-handler.ts:739`) and the app-page template (`build/templates/app-page-runtime.ts`, for example lines 1804, 1941 and 2154) pass `nextConfig.generateEtags` directly to the same function.
- The actual effect is in `server/send-payload.ts:64-71`:
  - The payload is `null` if the result is dynamic (streamed). Otherwise it is the unchunked string.
  - `if (generateEtags && payload !== null)` computes `generateETag(payload)`, a function in `server/lib/etag`. I did not read its implementation.
  - `sendEtagResponse` (`server/send-payload.ts:11-33`) then sets the `ETag` header. If `fresh(req.headers, { etag })` says the client's `If-None-Match` matches, it sets status 304, ends the response with no body, and returns.
- With `generateEtags: false`, none of that runs. Responses go out with no `ETag`, never get a 304 from this path, and always send the full body. `Content-Length` and the body or streaming are still handled afterwards (`server/send-payload.ts:73-92`).
- Streamed responses never get an ETag, whatever the setting, because `payload` is `null`.

**Static files**
- `server/lib/router-server.ts:665` passes `etag: config.generateEtags` to the static file sender, with the comment "Ensures that etags are not generated for static files when disabled." I did not open the function it passes this to. Its `etag` option is presumably what turns ETag generation on or off for those files.

**Uncertainty**
- I did not trace other call sites beyond the grep hits, or check any code that sets `ETag` independently of this option, such as route handlers or the image optimizer.