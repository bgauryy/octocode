**Short answer:** `redirect()` and `permanentRedirect()` don't return a response. They throw a special `Error` that carries the redirect in its `digest` string. The server catches that error at the render boundary and sets the status code and `Location` header from it. Route Handlers do the same thing in a separate catch path. I did not trace how Server Actions handle it beyond the entry point.

**1. Throwing** (`packages/next/src/client/components/redirect.ts`)
- `getRedirectError` (lines 10–18) creates `new Error(REDIRECT_ERROR_CODE)`. It sets `digest = \`${REDIRECT_ERROR_CODE};${type};${url};${statusCode};\``.
- `redirect()` (lines 33–41) throws that error with `RedirectStatusCode.TemporaryRedirect`. The type defaults to `'push'` inside a Server Action and `'replace'` otherwise.
- `permanentRedirect()` (lines 55–61) throws it with `RedirectStatusCode.PermanentRedirect`. The type defaults to `'replace'`.
- Helpers decode the digest:
  - `getURLFromRedirectError` (lines 70–77) takes `digest.split(';').slice(2, -2).join(';')`.
  - `getRedirectTypeFromError` (lines 79–85) takes `digest.split(';', 2)[1]`.
  - `getRedirectStatusCodeFromError` (lines 87–93) takes `Number(digest.split(';').at(-2))`.

**2. Page/RSC render** (`packages/next/src/server/app-render/app-render.tsx`)
- The error is caught and `isRedirectError(err)` is checked at line 4386, and again at 9839 in the second path.
- In both places the code sets `res.statusCode = getRedirectStatusCodeFromError(err)` (4388, 9841) and `metadata.statusCode`.
- It builds the URL with `addPathPrefix(getURLFromRedirectError(err), basePath)` (4391–4394, 9844).
- It then calls `setHeader('location', redirectUrl)` (4403, 9846).
- In the first path (4396–4401) it also copies cookies set via the cookies API into `set-cookie`.
- The same file contains a branch that throws the original error when it is neither a redirect nor an HTTP access-fallback error. That is the `cacheComponents` check at line 9852.

**3. HTML fallback for already-started streams** (`make-get-server-inserted-html.tsx:56–73`)
- If the error surfaces after streaming has begun, the server can't change the status code. It injects `<meta id="__next-page-redirect" http-equiv="refresh" content="0;url=…">` for permanent redirects, or `1;url=…` for temporary ones.

**4. Route Handlers** (`packages/next/src/server/route-modules/app-route/module.ts:424–450`)
- A caught redirect error becomes `new Response(null, { status, headers })` with a `Location` header (line 431). Mutable cookies are appended at line 437.
- The status is `303 SeeOther` if the request is a Server Action. Otherwise it is `getRedirectStatusCodeFromError(err)` (lines 445–447).

**5. Server Actions** (`action-handler.ts:1324–1326`)
- This file also catches `isRedirectError` and reads the URL and type from the error. I did not read the code after that point, so I can't describe how it builds the response.

**Uncertainty:** I read the code that sets the status and header, but I did not trace the upstream React Flight error handling that passes the error into these catch blocks.