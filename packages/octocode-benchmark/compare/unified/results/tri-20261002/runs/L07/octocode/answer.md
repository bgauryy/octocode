`redirect()` and `permanentRedirect()` don't write a response themselves. They throw a tagged error, and Next's server catches it and sets the status code and `Location` header. The paths below are all under `packages/next/src`.

**1. Throwing the error** (`client/components/redirect.ts`)
- `getRedirectError` (`:10-18`) builds `new Error(REDIRECT_ERROR_CODE)`. It sets `digest = "NEXT_REDIRECT;<type>;<url>;<statusCode>;"`.
- `redirect()` (`:33-41`) throws it with status 307 (`TemporaryRedirect`). Its type defaults to `push` inside a server action and `replace` otherwise.
- `permanentRedirect()` (`:55-61`) throws it with status 308 (`PermanentRedirect`). Its type defaults to `replace`.
- The helpers `getURLFromRedirectError` (`:70-77`) and `getRedirectStatusCodeFromError` (`:87-93`) parse the url and status back out of the digest.

**2. Catching it, by context**
- **Page or layout render** (`server/app-render/app-render.tsx:4386-4403`):
  - On `isRedirectError(err)`, it sets `res.statusCode = getRedirectStatusCodeFromError(err)`.
  - It copies any mutable cookies into a `set-cookie` header.
  - It sets `location` to the url with the `basePath` prefix added.
  - A second, near-identical branch is at `:9839-9846`; it does not set cookies.
  - The error-recovery render continues after this. I did not read that part, so I can't say what body is sent.
- **HTML body fallback** (`server/app-render/make-get-server-inserted-html.tsx:56-73`):
  - It injects `<meta id="__next-page-redirect" http-equiv="refresh" content="0;url=…">`, using 0 for permanent redirects and 1 otherwise.
  - Clients that miss the HTTP status, for example when the response has already started streaming, can still follow the redirect.
- **Route Handlers** (`server/route-modules/app-route/module.ts:424-449`):
  - It returns `new Response(null, { status, headers: { Location } })`.
  - The status is 303 if `actionStore.isAction`, otherwise the status from the error (307 or 308).
  - Mutable cookies are appended to the headers.
- **Server Actions** (`server/app-render/action-handler.ts:1324-1357`):
  - A fetch action keeps status 200. The redirect is sent through `x-action-redirect`, and the body is built by `createRedirectRenderResult` (`:1337`). I did not read what sets that header.
  - A non-JS (MPA) form post gets a 303 with a `Location` header and an empty body.

**Uncertainty:** I read the catch sites above but not the rest of the render flow after them. I also did not confirm that the `:9839` branch is the only other place the page path handles redirects.