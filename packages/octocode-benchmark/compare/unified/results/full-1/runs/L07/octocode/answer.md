**Short answer:** `redirect()` and `permanentRedirect()` don't build a response. They throw a special error whose `digest` string holds the URL and status code. The server catches that error at each entry point (page render, route handler, server action) and sets the status code and `Location` header from it. I read the code below. I did not read the server-action handler at `action-handler.ts:1325`, and I did not trace how the meta-tag fallback is emitted.

**1. Throwing** (`packages/next/src/client/components/redirect.ts`)
- `getRedirectError(url, type, statusCode)` creates `new Error(REDIRECT_ERROR_CODE)`. It sets `error.digest = \`${REDIRECT_ERROR_CODE};${type};${url};${statusCode};\``.
- `redirect()` throws that error with `RedirectStatusCode.TemporaryRedirect`. The type defaults to `'push'` inside a server action and `'replace'` otherwise.
- `permanentRedirect()` throws it with `RedirectStatusCode.PermanentRedirect`, and the type defaults to `'replace'`.
- Helpers decode the digest:
  - `getURLFromRedirectError` returns `digest.split(';').slice(2, -2).join(';')`.
  - `getRedirectStatusCodeFromError` returns `Number(digest.split(';').at(-2))`.
  - `getRedirectTypeFromError` returns the second digest field.
- Nothing in the file says which numeric values `TemporaryRedirect` and `PermanentRedirect` hold. The doc comments say 307 and 308.

**2. Catching and turning it into a response** (under `packages/next/src/server/`)
- **Page rendering** (`app-render/app-render.tsx`):
  - In the error path around lines 9835–9846, `isRedirectError(err)` sets `res.statusCode = getRedirectStatusCodeFromError(err)` and `metadata.statusCode`.
  - It then calls `setHeader('location', addPathPrefix(getURLFromRedirectError(err), basePath))`. So the `basePath` is prepended to the URL.
  - The same pattern appears around lines 4380–4400. There, mutable cookies are also copied into a `set-cookie` header via `appendMutableCookies`.
  - `errorType` is set to `'redirect'`, and rendering continues into the error-render path. The code I read stops there; I didn't read the rest.
- **Route handlers** (`route-modules/app-route/module.ts`, `handleHandlerError`, ~line 418):
  - If `isRedirectError(err)`, it reads the URL and throws an invariant error if the URL is empty.
  - It builds `new Headers({ Location: url })` and appends mutable cookies.
  - It returns `new Response(null, { status: ... })`. The status expression is cut off at `actionStore.isAction` in my read. The comment above it says an action gets a 303 so the POST isn't re-submitted.
- **Server actions** (`app-render/action-handler.ts:1325`): calls `getURLFromRedirectError(err)`. I did not read this code.
- **Inserted HTML** (`app-render/make-get-server-inserted-html.tsx:58`): also uses the URL from the error. This is presumably the meta-tag redirect for streamed pages, but I didn't read it.

**Uncertainty:** I did not read the tail of the route-handler status expression or the action-handler code. The 307, 308 and 303 values come from the doc comments in `redirect.ts` and the code comment in `module.ts`, not from a value I saw.