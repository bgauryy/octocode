Load when a boilerplate may hold more of the requested features than the framework's own CLI: the leading boilerplates per kind of project, and how to judge one. [Creational CLIs and boilerplates](boilerplates.md) holds the order of preference and the ways to create.

# Leading boilerplates

Stars and push dates below were read from GitHub on 2026-11-26. Re-check them, and read the boilerplate's README and `package.json` at its default branch before choosing it ([research](research.md#recipe), steps 3–5).

| Building | Boilerplate or CLI | Stars, last push | Start |
|---|---|---|---|
| Next.js SaaS (auth, billing, dashboard) | nextjs/saas-starter (Vercel; Postgres, Stripe, shadcn/ui) | 16.3k, 2025-12 | `--example https://github.com/nextjs/saas-starter` |
| | ixartz/SaaS-Boilerplate (auth, multi-tenancy, roles, i18n, tests) | 7.5k, 2026-11 | `git clone --depth 1` |
| | boxyhq/saas-starter-kit (enterprise SSO, audit logs) | 5.0k, 2026-07 | `git clone --depth 1` |
| | mickasmt/next-saas-stripe-starter (Next.js 16, Better Auth, Drizzle, Stripe) | 3.0k, 2026-09 | `git clone --depth 1` |
| Type-safe full stack | create-t3-app (Next.js, tRPC, Drizzle or Prisma, auth) | CLI 7.40, last release 2025-11 | `npm create t3-app@latest` |
| | Better-T-Stack (pick frontend, backend, ORM, auth) | CLI 3.49 | catalog: [web](stacks-web.md) |
| Web + mobile monorepo | t3-oss/create-t3-turbo (Next.js, Expo, tRPC, Drizzle) | 6.1k, 2026-11 | `npx create-turbo@latest -e https://github.com/t3-oss/create-t3-turbo` |
| | convex/turbo-expo-nextjs-clerk-convex-monorepo | 0.4k, 2026-11 | `git clone --depth 1` |
| React Native | infinitered/ignite (CLI, generators, Expo) | 20.0k, 2026-11 | `npx ignite-cli@latest new <Name> --yes` |
| | roninoss/create-expo-stack (navigation, styling and backend choices) | 2.6k, 2026-09 | `npx rn-new@latest` (the package's current name for the same CLI) |
| | obytes/react-native-template-obytes (Expo, expo-router, TanStack Query, EAS, CI) | 4.4k, 2026-10 | `git clone --depth 1` |
| Electron | electron-react-boilerplate | 24.2k, 2026-10 | `git clone --depth 1 --branch main https://github.com/electron-react-boilerplate/electron-react-boilerplate.git <dir>` |
| | cawa-93/vite-electron-builder (security-focused, any framework) | 3.0k, 2026-08 | `git clone --depth 1` |
| | electron-vite (`@quick-start/create-electron`), Electron Forge (`create-electron-app`) | CLIs | catalog: [apps](stacks-apps.md) |
| Tauri | kvnxiao/tauri-nextjs-template (Tauri 2, Next.js static export, CI) | 0.7k, 2026-11 | `git clone --depth 1` |
| Full stack in Python | fastapi/full-stack-fastapi-template (FastAPI, React, SQLModel, Postgres, Docker Compose) | 46.3k, 2026-11 | "Use this template", or `git clone --depth 1` |
| AI agent backend | vstorm-co/full-stack-ai-agent-template (FastAPI + Next.js, agents, RAG) | 2.0k, 2026-11 | Read its generator in the README |
| Browser extension | WXT, Plasmo (`pnpm create plasmo`; last release 0.90.5, 2025-05), chibat/chrome-extension-typescript-starter (2.8k) | — | catalog: [web](stacks-web.md) |
| Headless CMS or commerce | Payload (`npx create-payload-app`), Medusa (`npx create-medusa-app`), Strapi (`npx create-strapi`), Wix Headless templates ([backends](integrations-backends.md#headless-business-backends-cms-and-commerce)) | CLIs | Payload's docs on main install `@beta`: pin the release line |
| Docs site | Starlight (`--template starlight`), Docusaurus (`npx create-docusaurus@latest <dir> classic`), VitePress (`npx vitepress init`), Docus (`npx create-docus`) | CLIs | |
| UI on an existing React app | shadcn/ui (`npx shadcn@latest init`, then `add <component>`) | CLI | Copies components into your source |
| Other frameworks | RedwoodSDK (`create-rwsdk`), Qwik (`create-qwik`), Analog for Angular (`create-analog`), Vike (`create-vike`), Remotion video (`npx create-video@latest`) | CLIs | Read `--help` before running. LlamaIndex's `create-llama` is retired: [superseded](boilerplates.md#superseded-do-not-start-new-projects-with-these) |

## Judge a boilerplate

- Its last push is recent and it is not archived.
- Its `package.json` (or manifest) is on the major versions you plan to install.
- Its license fits the project.
- Its issues show no unresolved breakage.
- It covers more of the requested features than the official CLI.

Remove the features the user did not ask for, then run the same gates as for any scaffold.
