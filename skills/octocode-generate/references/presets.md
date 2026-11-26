Load when choosing the default stack for what the user is building, after the [intake](intake.md) answers and before research.

# Stack presets

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
