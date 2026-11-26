# Libraries

Load when a feature in scope needs a library; state, data fetching, forms, UI and mobile libraries are in [frontend libraries](libraries-frontend.md). Add one only for an in-scope feature, and prefer the framework's built-in or the platform (`fetch`, `node --env-file`, Go `net/http`) when it is enough. Majors as of 2026-11-26: run `check-versions.mjs` on the chosen set, which also flags deprecations (for example `lucia`), and read the official example for the integration ([research](research.md#recipe), step 4).

## HTTP servers and RPC

| Library | Use when | Avoid when |
|---|---|---|
| Hono 4 | Edge or multi-runtime (Workers, Bun, Deno, Node.js); small, typed RPC client | You depend on Express middleware |
| Fastify 5 | Node.js services needing throughput, JSON-schema validation, plugins | Edge runtimes |
| Express 5 | The largest middleware ecosystem, team familiarity | Performance or typed contracts lead |
| NestJS 12 | Large teams wanting modules, DI and conventions | Small services |
| Elysia | Bun-only services with end-to-end types | Node.js or edge targets |
| tRPC 11 / oRPC | TypeScript client and server in one repository; oRPC when you also need OpenAPI | Public APIs for other languages |
| Other languages | Rust: axum 0.8 + tokio. Python: FastAPI; Litestar as an alternative. Go: `net/http` routing; chi for middleware composition | — |

HTTP clients: built-in `fetch` by default; ky 2 adds retries, hooks and timeouts; ofetch suits Nuxt; axios when interceptors or legacy compatibility matter. Rust reqwest, Python httpx, Go `net/http`.

## Validation

Zod 4 by default (widest integration, Standard Schema). Valibot for the smallest bundles, ArkType for fast checks with TypeScript-like syntax, TypeBox for JSON Schema output. Python: Pydantic 2. Validate every input at the trust boundary: request, env, file.

## Data

| Library | Use when |
|---|---|
| Drizzle ORM | SQL-first, light, edge-friendly; migrations with drizzle-kit |
| Prisma | Schema-first with a generated client and migrations. Check the version: `latest` can be a release candidate (8.0.0-rc.35 on 2026-11-26), and Better Auth's adapter declares Prisma 5–7 |
| Kysely | Type-safe query builder without an ORM |
| Mongoose | MongoDB |
| Rust | sqlx (compile-checked SQL), SeaORM or Diesel |
| Python | SQLAlchemy 2 + Alembic; SQLModel with FastAPI |
| Hosted backends | Supabase, Convex or Firebase ([backends](integrations-backends.md#databases-and-backend-platforms)): when you want managed data and auth over running a database |

## Auth

Better Auth is the default for self-hosted TypeScript auth; Auth.js is now part of Better Auth, which recommends Better Auth for new projects (`next-auth@latest` is still 4.x). Lucia is deprecated. Hosted providers (Clerk, Auth0, WorkOS, Supabase Auth; [integrations](integrations.md#payments-auth-email-and-ai)) when you want managed users, SSO or compliance. Keep sessions server-side and secrets in env, and rate-limit auth routes.

## AI

AI SDK for streaming chat, tools and structured output in TypeScript; Mastra for agents, workflows and memory. Packages, scaffold commands and the MCP SDKs: [services](stacks-services.md#ai-apps-agents-and-mcp-servers).

## Operations

Logging: pino (Node.js), tracing (Rust), structlog or stdlib logging (Python), `log/slog` (Go). Env: validate at startup with the schema library.
