# Next.js — 10 Research Questions

10 research questions about `vercel/next.js` and the surrounding Next.js benchmark/eval ecosystem. Answer using whatever tooling you have available.

---

## Section 1 — Remote Only (Q1–Q10)

> **MUST:** Answer Q1–Q10 using **remote GitHub repositories only**. Do **not** clone any repo for this section.

---

### Q1 — How does `notFound()` propagate to the not-found boundary?

In `vercel/next.js`, trace how `notFound()` interrupts App Router rendering and triggers the nearest `not-found.tsx` segment.
1. Where is `notFound()` defined? State the exact file path and line number.
2. What does it throw? Quote the relevant source line.
3. Where is that thrown value caught and converted into `not-found` segment rendering? State the file path and function name.

> *Tests multi-file code tracing. D=3 requires exact file:line for the definition, a verbatim quote of the throw mechanism, and the correct catch site. Agents that stop at the public re-export without finding the internal throw + catch chain score D≤1.*

---

### Q2 — Bulk symbol definition lookup

Find where each of these symbols is **defined** (class or function declaration, not an import) in `vercel/next.js`:
- `NextRequest` — exact file path and line number
- `NextResponse` — exact file path and line number
- `ImageResponse` — exact file path and line number

> *Tests multi-target lookup. D=3 requires exact file:line for all three — not just filenames. Agents that return import sites instead of declaration sites score D≤1.*

---

### Q3 — `revalidatePath` call sites [drift]

In `vercel/next.js`, find every call to `revalidatePath` inside `packages/next/src/server/`.
For each match: state the file path, line number, and the exact source line.

> *Tests search completeness and match-context quality. Judge verifies that line numbers are correct and that returned source lines match the actual file. D=3 requires exact source lines for every match, not paraphrases. Judge independently verifies the total count.*

---

### Q4 — Files referencing both routers

In `vercel/next.js`, find files under `packages/next/src/` that contain **both** `appDir` and `pagesDir` in the same file.
1. How many files match?
2. List all file paths.

> *Tests file-level AND-intersection. A result set that includes files containing only one of the two terms will overcount. Judge verifies the correct count independently.*

---

### Q5 — How does `redirect()` work in Server Components?

In `vercel/next.js`, trace how the `redirect()` function works end-to-end in the App Router — from the call site in a Server Component through to the HTTP response.
1. Where is `redirect()` defined? State the exact file path and line number.
2. What mechanism does it use to interrupt rendering? Quote the relevant source line(s).
3. Where is that signal caught and converted into an HTTP redirect response? State the file path and function name.

> *Tests multi-hop code tracing. D=3 requires exact file:line for the definition, a verbatim quote of the interruption mechanism, and the correct catch/response site. Agents that stop at the public re-export without tracing the internal throw + catch chain score D≤1.*

---

### Q6 — `renderToHTMLOrFlight` signature

Read `packages/next/src/server/app-render/app-render.tsx` in `vercel/next.js`.
1. What does `renderToHTMLOrFlight` return? State the exact return type.
2. List its parameters by name and type.
3. What is the first thing the function does before any rendering work? Quote the relevant line.

> *Tests targeted reads on a multi-thousand-line file. D=3 requires the exact return type, all parameter names + types, and a verbatim quote of the opening logic — not a summary.*

---

### Q7 — How does `revalidateTag` invalidate cached data?

In `vercel/next.js`, trace how calling `revalidateTag(tag)` invalidates cached entries end-to-end.
1. Where is `revalidateTag` defined on the server side? State the exact file path and line number.
2. What data structure does it write to when called? Quote the relevant source line(s).
3. Where does the server read that structure to decide what to revalidate? State the file path and function name.

> *Tests multi-hop architecture tracing through the cache invalidation pipeline. D=3 requires file:line for the definition, a verbatim quote of the write operation, and the correct consumer location. Agents that find only the public export without tracing the cache store write + consumer read score D≤1.*

---

### Q8 — How does a Server Action request reach the server?

In `vercel/next.js`, trace how an HTTP request carrying a Server Action is identified and routed on the server.
1. What HTTP header does Next.js use to identify a Server Action request? State the file path and the exact header name or constant.
2. Which function handles the Server Action execution? State its file path and name.
3. How does Next.js return the action result to the client? Quote the relevant response-building line.

> *Tests cross-layer architecture tracing from HTTP edge to server execution. D=3 requires the correct header/constant name with file:line, the correct executor function with location, and a verbatim response-building quote.*

---

### Q9 — Official Next.js agent eval benchmark

Find Vercel's public GitHub repository for official **Next.js AI agent evaluations**.
1. State the repository name and quote what the README says the benchmark evaluates.
2. What files make up one eval case? List the required filenames and the role of each file.
3. List the current eval IDs that test caching, proxy/middleware, request APIs, and revalidation behavior.

> *Tests repository discovery plus benchmark-structure extraction. D=3 requires the correct repo, README-backed quote, exact eval-case file roles, and relevant eval IDs. Agents that only cite nextjs.org/evals without finding the GitHub repository score D≤1.*

---

### Q10 — Official Turbopack benchmark workflow

In `vercel/next.js`, find the GitHub Actions workflow that runs the official **Turbopack benchmarks**.
1. State the workflow file path and workflow name.
2. What events and path filters trigger it? Quote the relevant YAML lines.
3. List every benchmark job and, for each job, the Cargo package/bench target it builds or runs.

> *Tests workflow discovery and benchmark-harness reading. D=3 requires exact workflow path/name, trigger filters, all benchmark jobs, and the Cargo commands. Agents that find a generic CI workflow instead of the Turbopack benchmark workflow score D≤1.*

---
