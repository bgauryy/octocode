# Intake

Load before choosing anything. Ask in batches of at most four questions, each with 2–4 options and a recommended default, and skip any question the request or [detect-workspace](../scripts/detect-workspace.mjs) already answers. Stop asking once every row below is answered or delegated ("you choose" means: recommend, state the assumption, record it in the plan).

## Batch 1 — what

| Ask | Why it decides |
|---|---|
| What does it do, for whom? One sentence plus the 3–5 core features | App type and scope; more than one app means a monorepo question |
| Where does it run? Browser, server, edge, mobile, desktop, CLI, library | Framework and runtime family |
| Rendering and data: static, SSR, SPA, API only, realtime | Framework mode; server or client state |
| Existing code, design, API spec or reference app to match? | Conventions to follow; research seeds |

## Batch 2 — how

| Ask | Why it decides |
|---|---|
| Language and stack preference, or "you choose" | Skips research for settled choices |
| Package manager and runtime (npm, pnpm, Yarn, Bun; Node version) when none is detected | [package managers](package-managers.md) |
| Lint/format lane: framework default, ESLint + Prettier, Oxlint + Oxfmt, Biome | [toolchain](toolchain.md) |
| Deployment target and constraints: Vercel, Cloudflare, AWS, Docker, self-hosted; budget, compliance | Adapters, runtime limits, CI |

## Batch 3 — features (only those in scope)

| Ask | Options source |
|---|---|
| Data: which database, ORM or query builder, migrations | [libraries](libraries.md#data) |
| Auth: providers, sessions, roles | [libraries](libraries.md#auth) |
| State, forms, styling/UI kit | [libraries](libraries.md#client-state) |
| Testing depth: unit only, plus component, plus end-to-end; coverage floor | [toolchain](toolchain.md#test) |

## Batch 4 — delivery

| Ask | Default when unanswered |
|---|---|
| Target folder and project name | Folder from the request; name from the folder |
| CI provider and git hooks | GitHub Actions from the templates; no hooks |
| Git: init a repository? (never commit or push unasked) | Init only in a new, empty folder |
| Autonomy: approve the plan once, or confirm each phase | Approve once; stop on any gate failure |

Record every answer and assumption in the plan's Requirements table, marking each as user, detected or assumed.
