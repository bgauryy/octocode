# Intake

Load for the first reply. Aim for one question round, then the design and plan. Ask through the host's question tool (AskUserQuestion or its equivalent): a few questions per call, 2–4 options each with the recommended one first, multi-select for lists such as services, and free text for the one-sentence idea. Without such a tool, ask the same questions in one short message. Skip what the request, the repository or `detect-workspace.mjs` already answers.

## Ask what first

| Ask | Why it matters |
|---|---|
| What are you building, for whom, and its core features (a sentence plus a short list) | Preset, scope, data and auth |
| Where it runs: web, iOS/Android, desktop, server/API, edge, CLI, library, browser extension | Platform catalog |
| Target folder and project name (default: a new folder `<slug>` in the current one) | Where the scaffold runs; it must be new or empty |
| Mocks or references to build from: designs or screenshots, a Figma export, an API spec, sample data, a site or app to resemble | The build follows them instead of guesses |
| Hard constraints: a required language, framework, host or existing backend (Firebase, Supabase, Wix, Base44, MongoDB); budget or compliance | Which preset or alternative |
| Recommended stack, or choose each piece | Whether a second round is needed |

Save every file or link the user gives to `<workspace>/.octocode/octocode-generate/<slug>/context/`, one file per item plus a `README.md` that lists each with a line on what it shows. An image pasted into the chat may not exist as a file: describe it in `context/README.md` (screens, text, colors, layout) and ask for the file when exact assets matter (logos, icons). Read images with the host's image support, or `clasify` with the image as an asset. Read reference sites with `clasify` `source:{"url":…}` or the host's web tools. Note in the plan what each item decided: screens, fields, flows, style.

## When to ask more

- **Second round.** Only when the user wants to choose each piece, or when a costly choice stays open: data store, auth, a hosted backend, or the package manager when none is detected. Offer the leading options from [integrations](integrations.md) with their docs links.
- **Services.** Ask once, with defaults off, whether the app needs analytics, error monitoring, payments or email. Tracking needs the user's yes.
- **Brainstorm.** When the idea is still open (no clear features, or the user asks for ideas), offer a short brainstorm before planning: the `octocode-brainstorming` skill when installed, otherwise 3–5 directions with a one-line trade-off each. Offer it; do not force it.

Every other choice takes the preset default, recorded as `assumed`.

## Presets

The default wins unless an alternative's condition holds. Take exact commands from the catalog row and confirm them by [research](research.md) before planning.

When two rows fit (a booking SaaS fits both "SaaS" and "bookings"), the user's named tools decide: Stripe, multi-tenancy or your own database mean the SaaS row; wanting a hosted business backend with its own payments, bookings and dashboard means Wix Headless. Still tied: ask, with the deciding difference as each option's description.

| Building | Default | Alternative → when |
|---|---|---|
| SaaS or full-stack web app | Next.js (App Router, TypeScript, Tailwind) + Drizzle + Postgres + Better Auth, or a SaaS boilerplate that already has billing and teams ([boilerplates](boilerplates.md)) | TanStack Start → end-to-end typed routes and server functions; React Router → web-standard loaders on any host; SvelteKit or Nuxt → a Svelte or Vue team |
| Store, bookings, events or CMS site | Wix Headless on its official Next.js or Astro template | Shopify Hydrogen → commerce on Shopify; Sanity or Contentful → content only |
| App on an existing backend platform | The platform's SDK in the matching frontend: Base44 (`@base44/sdk`), Firebase, Supabase | — |
| Web, API and mobile sharing code | Turborepo with Next.js and Expo apps and shared packages | Better-T-Stack → choose frontend, backend, ORM and auth in one command |
| Marketing site, blog, docs | Astro (Starlight for docs) | Next.js → the site becomes an app |
| SPA or dashboard on an existing API | Vite `react-ts` + TanStack Router + TanStack Query | Angular → a large team wanting one opinionated framework |
| Batteries-included web monolith | Rails 8 | Laravel 13 → PHP; Django 6 → Python with an admin; Phoenix → realtime on Elixir |
| Mobile app (iOS and Android) | Expo with Expo Router; Firebase or Supabase when it needs a managed backend | Flutter → one Dart codebase that also targets desktop and web; SwiftUI or Jetpack Compose → platform APIs dominate; Capacitor → wrap an existing web app |
| Desktop app | Tauri 2 with a React TypeScript frontend | electron-vite → Node APIs in the main process; Flutter → shared with a mobile app; .NET MAUI → a .NET team |
| Full-stack app with one runtime | Bun: `bun init --react`, with `Bun.serve` routes for pages and API and built-in `sql` | Next.js → SSR, a framework ecosystem and hosting adapters |
| HTTP API or service | Hono on Node.js (the same code runs on Bun, Deno and Workers) | FastAPI → Python; Go `net/http` → one static binary; axum → Rust footprint and latency; ASP.NET Core → .NET; Spring Boot or Ktor → JVM; NestJS → a large TypeScript team wanting modules and DI |
| Edge function or worker | Cloudflare Workers through create-cloudflare, with Hono | Fresh on Deno → Deno Deploy |
| CLI tool | The team's language; otherwise Rust + clap (one static binary) | Go + cobra; Python + Typer; TypeScript on Bun → one executable with `bun build --compile`; TypeScript + tsdown → npm distribution |
| Library or package | TypeScript with tsdown | `cargo new --lib`; `uv init --lib`; `swift package init --type library`; `dotnet new classlib` |
| Browser extension | WXT | — |
| AI app or agent | Next.js + AI SDK | Mastra → multi-step agents and workflows; Python → the team's ML stack |
| MCP server | TypeScript with `@modelcontextprotocol/server` (SDK v2) | Python `mcp` SDK (`MCPServer`) or FastMCP |
| Game | Phaser Vite TypeScript template (2D, web) | Bevy → Rust, 2D or 3D, native |

## Approve

After `design.md` and `plan.md` exist, show their paths and a short summary (stack, screens, integrations, commands that will run, open questions), then ask through the question tool:

| Option | Then |
|---|---|
| Approve and build (recommended) | Record the approval in `plan.md`; start building |
| Change the design | Revise `design.md` and the plan rows it affects; ask again |
| Change the stack or services | Revise `plan.md`, re-run the affected research; ask again |

Building starts only on an explicit approval.

## Record

Write each answer to the plan's Requirements table marked `user`, `detected` or `assumed`, and list the context items. An assumption the user later rejects reopens only the decisions that rest on it.
