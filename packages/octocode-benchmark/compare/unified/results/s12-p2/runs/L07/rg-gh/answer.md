**Short answer:** `redirect()` and `permanentRedirect()` don't write a response. They throw a special error whose `digest` string holds the redirect details. Whichever server layer catches it (page render, route handler, or server action) detects it with `isRedirectError`. That layer then sets the status code and `Location` header on the response. All paths are under `packages/next/src`.

**1. Throwing the error**
- `getRedirectError` builds `new Error('NEXT_REDIRECT')` and sets `error.digest = "NEXT_REDIRECT;<type>;<url>;<status>;"` (`client/components/redirect.ts:9-17`).
- `redirect()` throws it with status 307 (`redirect.ts:~38-40`). Its type defaults to `push` inside a server action and `replace` otherwise.
- `permanentRedirect()` throws it with status 308 and type `replace` (`redirect.ts:~58-63`).
- The status codes come from the enum 303 / 307 / 308 in `client/components/redirect-status-code.ts`.

**2. Recognizing the error**
- `isRedirectError` in `client/components/redirect-error.ts:19-41` parses the digest. It checks that the code is `NEXT_REDIRECT`, the type is `push` or `replace`, and the status is in `RedirectStatusCode`.
- `getURLFromRedirectError`, `getRedirectStatusCodeFromError` and `getRedirectTypeFromError` read the fields back out of the digest (`redirect.ts:~70-100`).

**3. Turning it into a response**
- **Page and RSC render** (`server/app-render/app-render.tsx`):
  - In the catch block at ~4386-4403, it sets `res.statusCode = getRedirectStatusCodeFromError(err)`. It prefixes the URL with `basePath` via `addPathPrefix`, copies any mutable cookies into `set-cookie`, and calls `setHeader('location', redirectUrl)`.
  - A second, similar path at ~9833-9846 sets the status and the `location` header the same way. That path covers the case where an error occurs after the RSC prerender.
  - If the redirect happens after the response has started streaming, the status and headers are already sent. In that case `make-get-server-inserted-html.tsx:56-70` injects `<meta id="__next-page-redirect" http-equiv="refresh" content="0|1;url=...">` into the HTML. It uses `0` for a 308 and `1` otherwise.
- **Route handlers** (`server/route-modules/app-route/module.ts:424-449`):
  - It returns `new Response(null, { status, headers: { Location: url } })`, with mutable cookies appended.
  - The status is the digest's 307/308, but 303 if `actionStore.isAction`.
  - It throws an invariant error if the URL is empty.
- **Server actions** (`server/app-render/action-handler.ts:1324-1357`):
  - For a non-fetch (MPA/form) action, it sets status 303 and the `Location` header with an empty body (`:1350-1357`).
  - For a fetch action, the status stays 200 and the redirect is carried in the `x-action-redirect` header. The body can include the target's Flight response (`createRedirectRenderResult`, ~1327-1346). I did not read that function.

**Uncertainty:** the `redirect.ts` line numbers marked `~` are approximate. I did not trace the client-side handling of the meta tag or the `x-action-redirect` header.