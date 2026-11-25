# Frameworks

Load when choosing the app framework. Commands were current on 2026-11-25; confirm the CLI's current version and flags with `check-versions.mjs <package>` and `<command> --help` before running it, and prefer non-interactive flags.

| Need | Framework | Scaffold | Use when | Avoid when |
|---|---|---|---|---|
| React, full stack | Next.js | `npx create-next-app@latest` | SSR/SSG/ISR, server components, Vercel-first | You want a plain SPA or a minimal server |
| React, full stack, web standards | React Router (framework mode, formerly Remix) | `npx create-react-router@latest` | Loaders/actions, progressive enhancement, any host | You need RSC-heavy Next features |
| React, full stack, type-safe routing | TanStack Start | `npm create @tanstack/start@latest` | End-to-end typed routes and server functions | You need a long-stable API surface |
| Type-safe full stack starter | T3 Stack | `npx create-t3-app@latest` | Next.js + tRPC + Prisma/Drizzle + auth in one go | You will replace most of its picks |
| Vue, full stack | Nuxt | `npx nuxi@latest init` | Vue with SSR and file routing | — |
| Svelte, full stack | SvelteKit | `npx sv create` | Small bundles, simple reactivity | Team is React-only |
| Content sites | Astro | `npm create astro@latest` | Docs, blogs, marketing; islands of any UI library | App-like state across pages |
| SolidJS | SolidStart | `npm create solid@latest` | Fine-grained reactivity, small runtime | Ecosystem breadth matters |
| SPA | Vite | `npm create vite@latest` | React/Vue/Svelte/Solid SPA or library playground | You need SSR |
| SPA with unified toolchain | Vite+ | `vp create` | One CLI for dev, check (fmt+lint+types), test, build, monorepo tasks | Team must keep ESLint plugins or a custom toolchain |
| Enterprise SPA | Angular | `npx @angular/cli new` | Opinionated, batteries included, large teams | Small apps |
| Mobile | Expo | `npx create-expo-app@latest` | React Native, iOS + Android, OTA updates | — |
| Desktop, web tech, small binary | Tauri | `npm create tauri-app@latest` | Rust backend, system webview, small installers | You need Node APIs in the main process |
| Desktop, Electron | electron-vite | `npm create @quick-start/electron@latest` | Full Node + Chromium, Vite dev loop | Binary size matters |
| Node API, enterprise | NestJS | `npx @nestjs/cli new` | Modules, DI, decorators, large teams | Small services |
| Node API, fast | Fastify | `npx fastify-cli generate` | Schema validation, plugins, throughput | — |
| API on any runtime | Hono | `npm create hono@latest` | Edge (Workers), Bun, Deno, Node with one codebase | You need Express middleware |
| Library | tsdown | `npx create-tsdown@latest` | Publishing TypeScript packages (ESM + d.ts) | Apps |
| JS monorepo | Turborepo / Nx | `npx create-turbo@latest` / `npx create-nx-workspace@latest` | Several apps/packages with cached tasks | One package |
| Rust | Cargo | `cargo new <name>` (`--lib`), `cargo generate <template>` | CLIs, services (axum), libraries | — |
| Python | uv | `uv init <name>` (`--package` for src layout, `--lib`) | FastAPI, Django, CLIs (Typer), data | — |
| Go | Go modules | `go mod init <module>` | Services, CLIs; stdlib `net/http` routing first | — |

Create React App is deprecated (react.dev, 2025-02-14): never scaffold with it.

## Choose

1. Filter by where it runs and rendering mode from the intake.
2. Prefer the framework's own CLI and defaults; every replacement of a default names the requirement that beats it.
3. Keep two candidates to the end: compare them on the deciding constraint (hosting, team skill, SSR need, ecosystem) and record the loser and why in the plan's Decisions table.
4. In a monorepo, follow the existing framework and conventions unless the user asks otherwise.
