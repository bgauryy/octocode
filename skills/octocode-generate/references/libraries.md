# Libraries

Load when the intake names a feature that needs a library. Add a library only for a feature in scope; prefer the framework's built-in or the platform (`fetch`, `node --env-file`, Go `net/http`) when it suffices. Versions as of 2026-11-25: run `check-versions.mjs` on the chosen set, which also flags deprecations (for example `lucia`).

## HTTP servers

| Library | Use when | Avoid when |
|---|---|---|
| Hono 4 | Edge or multi-runtime (Workers, Bun, Deno, Node); small, typed RPC client | You depend on Express middleware |
| Fastify 5 | Node services needing throughput, JSON-schema validation, plugins | Edge runtimes |
| Express 5 | Largest middleware ecosystem, team familiarity, simple services | Performance or typed contracts lead |
| NestJS 12 | Large teams wanting modules, DI and conventions | Small services |
| Elysia | Bun-only services with end-to-end types | Node or edge targets |
| tRPC 11 / oRPC | TypeScript client and server in one repo; oRPC when you also need OpenAPI | Public APIs for other languages |
| Rust: axum 0.8 (+ tokio) | Default Rust web service | — |
| Python: FastAPI | Typed async APIs with OpenAPI; Django 6 for full-stack apps with admin and ORM; Litestar as a FastAPI alternative | — |
| Go: `net/http` | Stdlib routing with methods and path patterns covers most services; chi v5 for middleware composition; Gin or Echo when the team knows them | — |

## HTTP clients

Default to built-in `fetch` (browsers, Node, Bun, Deno). ky 2 adds retries, hooks and timeouts on top of `fetch`; ofetch suits Nuxt/UnJS; axios 1 when interceptors or legacy compatibility matter; undici for low-level Node pooling. Rust: reqwest. Python: httpx (sync and async). Go: `net/http`.

## Client state

| Need | Library | Use when |
|---|---|---|
| Server data (fetching, caching, invalidation) | TanStack Query 5; SWR for simple cases | Any remote data: do not put it in a global store |
| Small global UI state, React | Zustand 5 | Default store: tiny API, no provider |
| Atomic, derived state, React | Jotai | Many independent pieces of state |
| Large app, strict patterns, devtools | Redux Toolkit 2 (with RTK Query) | Big teams, event logs, existing Redux |
| Vue | Pinia | Default Vue store |
| Complex flows, workflows | XState 5 | Explicit state machines |
| Framework-agnostic | nanostores | Shared state across islands (Astro) or frameworks |

Start with local component state and URL state; add a store only when state is shared across distant components.

## Validation

Zod 4 is the default (widest integration). Valibot for the smallest bundles; ArkType for fast runtime checks with TS-like syntax; TypeBox for JSON Schema output. Python: Pydantic 2. Validate every input at the trust boundary (request, env, file).

## Data

| Library | Use when |
|---|---|
| Drizzle ORM | SQL-first, light, edge-friendly; migrations with drizzle-kit |
| Prisma | Schema-first with generated client and migrations; check the version: `latest` may be a release candidate |
| Kysely | Type-safe query builder without an ORM |
| Mongoose | MongoDB |
| Rust: sqlx (compile-checked SQL), SeaORM or Diesel | — |
| Python: SQLAlchemy 2 + Alembic; SQLModel with FastAPI | — |

## Auth

Better Auth is the default for self-hosted TypeScript auth; it now also maintains Auth.js (`next-auth@latest` is still 4.x). Lucia is deprecated. Hosted providers (Clerk, Auth0, WorkOS, Supabase Auth) when you want managed users, SSO or compliance. Store sessions server-side, keep secrets in env, and add rate limiting on auth routes.

## Forms, UI and the rest

Forms: React Hook Form; TanStack Form for framework-agnostic typed forms. Styling: Tailwind CSS 4; components: shadcn (copies source you own) on Radix primitives. Routing in SPAs: TanStack Router or React Router. Logging: pino (Node), tracing (Rust), structlog or stdlib logging (Python), `log/slog` (Go). Env: validate with the schema library at startup.
