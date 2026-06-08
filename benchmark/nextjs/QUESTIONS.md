# Questions

20 research questions about the `vercel/next.js` repository.

**External questions (Q1–Q10):** use GitHub API tools against `vercel/next.js`.
**Local questions (Q11–Q20):** clone the repo first — `git clone --depth 1 https://github.com/vercel/next.js /tmp/nextjs-bench` — and use `/tmp/nextjs-bench` as your local root.

---

### Q1 — Exhaustive `"use client"` directive coverage

In `vercel/next.js`, find every file under `packages/next/src/` that contains `'use client'` as an actual directive (the first statement in the file), excluding the `compiled/` subdirectory.
1. How many unique files contain this directive?
2. List all file paths.

---

### Q2 — Bulk symbol definition lookup

Find where each of these symbols is **defined** (class or function declaration, not an import) in `vercel/next.js`:
- `NextRequest` — exact file path and line number
- `NextResponse` — exact file path and line number
- `ImageResponse` — exact file path and line number

---

### Q3 — textMatch context: `revalidatePath` call sites

In `vercel/next.js`, find every call to `revalidatePath` inside `packages/next/src/server/`.
For each match: state the file path, line number, and the exact source line.

---

### Q4 — AND-intersection: files referencing both routers

In `vercel/next.js`, find files under `packages/next/src/` that contain **both** `appDir` and `pagesDir` in the same file.
1. How many files match?
2. List all file paths.

---

### Q5 — Large file targeted read: app-render entry

Read `packages/next/src/server/app-render/app-render.tsx` in `vercel/next.js`.
1. What are the names of all top-level exported functions or types?
2. What does `renderToHTMLOrFlight` return and what are its required parameters?

---

### Q6 — Directory listing with extension breakdown

List all files directly inside `packages/next/src/server/` in `vercel/next.js` (non-recursive, flat listing only).
1. How many total files are in this directory?
2. How many have `.ts` vs `.tsx` extensions?

---

### Q7 — Monorepo package enumeration

In `vercel/next.js`, list every directory directly under `packages/`.
1. How many top-level packages exist?
2. List all package directory names.

---

### Q8 — Feature archaeology: Partial Prerendering

Search merged PRs in `vercel/next.js` to find the PR that first introduced Partial Prerendering (PPR).
1. What is the PR number and title?
2. What was the stated motivation in the PR body?
3. Which key files were added or changed?

---

### Q9 — Inline review thread depth

In `vercel/next.js`, find a merged PR that introduced or significantly changed middleware execution in the App Router.
1. How many inline review comments (code-level thread comments, not PR-level summaries) does it have?
2. Which file received the most inline comments?
3. Quote the most substantive reviewer objection.

---

### Q10 — npm registry: Next.js ecosystem packages `[drift]`

Look up the following npm packages. For each report: current version, weekly download count, and repository URL.
- `next`
- `create-next-app`
- `@next/bundle-analyzer`

What replaced the deprecated `@next/font` package, and where does it now live?

---

### Q11 — `"use server"` directive frequency

Find every file under `packages/next/src/` that contains the exact string `"use server"`, excluding the `compiled/` subdirectory.
1. How many unique files contain it?
2. List all file paths.

---

### Q12 — TODO/FIXME annotation survey

Find every `// TODO`, `// FIXME`, or `// HACK` comment in `packages/next/src/server/`.
For each: state the file path, line number, and exact comment text.
What is the total count?

---

### Q13 — Largest source files by size

Find the 5 largest `.ts` or `.tsx` files by byte size under `packages/next/src/server/`.
For each: state the file path and file size.

---

### Q14 — Server directory structure

List all direct subdirectories under `packages/next/src/server/`.
1. How many subdirectories are there?
2. What does each appear to be responsible for based on its name?

---

### Q15 — Abstract class interface: `BaseServer`

Read `packages/next/src/server/base-server.ts`.
1. What is the stated purpose of the `BaseServer` class?
2. How many `abstract` methods does it declare? List their names and return type signatures.

---

### Q16 — Configuration defaults

Read `packages/next/src/server/config-shared.ts`.
1. What is the default value for `reactStrictMode`?
2. What is the default value for `poweredByHeader`?
3. List 5 other top-level config keys with their default values.

---

### Q17 — Symbol definition: `NextRequest`

Where is the `NextRequest` class defined (not re-exported)?
1. State the exact file path and line number of the class declaration.
2. What does `NextRequest` extend?

---

### Q18 — Type definition: `Metadata`

The main `Metadata` type lives under `packages/next/src/lib/metadata/types/`. Which file defines the root `Metadata` type or interface?
1. State the exact file name and line number of the type declaration.
2. List 5 of its optional fields with their value types.

---

### Q19 — Reference exhaustiveness: `unstable_cache`

Find all files across `packages/` that reference or import `unstable_cache`, excluding the `compiled/` subdirectory.
1. How many unique files reference it?
2. List all file paths grouped by package.

---

### Q20 — Incoming call hierarchy: `renderToHTMLOrFlight`

Trace the direct callers of the function `renderToHTMLOrFlight` in the server rendering pipeline.
1. List each direct caller: function name, file path, and line number of the call site.
2. How many direct callers does it have?

