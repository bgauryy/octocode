# Web, edge and monorepo stacks

Load when the project runs in a browser or on a web or edge server, or when it spans several JavaScript apps. Rows were verified on 2026-11-26 against each project's docs and CLI source. Before running a command, re-read the **Docs** path at the scaffolder's current release ([research](research.md)): flags change between minors. `<dir>` is the target folder and `<pm>` the package manager; [ways to create](boilerplates.md#ways-to-create) has the form for each package manager.

## Full-stack frameworks

| Framework | Non-interactive scaffold | Sets up | Watch | Docs |
|---|---|---|---|---|
| Next.js 16 | `npx create-next-app@latest <dir> --yes`, or explicit `--ts --app --tailwind --eslint` (or `--biome`, `--no-linter`) `--use-<pm> --skip-install --disable-git --no-agents-md` | TypeScript, Tailwind, ESLint, App Router, Turbopack, `@/*` alias, AGENTS.md; no test runner | `next build` no longer lints and `next lint` is gone: keep the `lint` script. Type-check with `"typecheck": "next typegen && tsc --noEmit"` (route types such as `LayoutProps` exist only after `next typegen`, `dev` or `build`). create-next-app 16.4 pins ESLint 9 and TypeScript 5 (its main branch already pins ESLint 10): keep the lint pins the release you ran generated. Add the `format-prettier` template and Vitest | `vercel/next.js/docs/01-app/01-getting-started/01-installation.mdx`, `vercel/next.js/packages/create-next-app/index.ts` |
| React Router 8 (framework mode) | `npx create-react-router@latest <dir> --yes --no-install --no-git-init --package-manager <pm> --no-agent-skills` | Tailwind and a `typecheck` script | No lint, format or test: add a lint lane and Vitest | `remix-run/react-router/docs/start/framework/installation.md` |
| TanStack Start | `npx @tanstack/cli@latest create <dir> -y --package-manager <pm> --no-git --no-install` (`--toolchain eslint` or `biome`, `--blank`, `--deployment cloudflare`) | Starter UI, Tailwind, devtools; linting only with `--toolchain` | `create-start-app` and `create-tsrouter-app` still publish as aliases of this CLI; TanStack's docs give `@tanstack/cli create` | `TanStack/cli/docs/cli-reference.md` |
| Nuxt 4 | `npm create nuxt@latest <dir> -- --template minimal --packageManager <pm> --gitInit --no-install` | Minimal app; modules only with `--modules a,b` | `npx nuxi init` runs the same initializer (nuxi 4.1); Nuxt's docs give `npm create nuxt@latest`. Without a TTY the folder, template, package manager and git choice are required | `nuxt/cli/packages/create-nuxt/src/init.ts` |
| SvelteKit | `npx sv create <dir> --template minimal --types ts --add eslint prettier vitest --install <pm>` (`--no-add-ons --no-install` for bare) | Only the add-ons you pass | Prompts unless `--add` or `--no-add-ons` is given | `sveltejs/cli/documentation/docs/20-commands/10-sv-create.md` |
| SolidStart 2 | `npm create solid@latest <dir> -- --solidstart --v2 --ts` | — | Needs Node.js 24+; the CLI does not install dependencies | `solidjs/solid-docs/src/routes/solid-start/v2/(1)getting-started.mdx`; flags: `solidjs-community/solid-cli/packages/create/src/index.ts` |
| Angular 22 | `npx @angular/cli@latest new <name> --package-manager=<pm> --style=css --ssr=false --zoneless --ai-config=none --skip-git --skip-install` | Vitest as the test runner; no linter | `ssr` and `zoneless` prompt unless passed | `angular/angular-cli/packages/schematics/angular/ng-new/schema.json` |
| Better-T-Stack | `BTS_TELEMETRY_DISABLED=1 npx create-better-t-stack@latest <dir> --yes --package-manager <npm, pnpm or bun> --no-install` (`--frontend`, `--backend`, `--addons biome`, `oxlint`, …) | The chosen frontend, backend, ORM and auth; linting only through `--addons` | Telemetry is on by default | `AmanVarshney01/create-better-t-stack/apps/cli/README.md` |

## SPA and content sites

| Framework | Non-interactive scaffold | Sets up | Watch | Docs |
|---|---|---|---|---|
| Vite 8 (create-vite 9) | `npm create vite@latest <dir> -- --template react-ts --no-interactive` (also `vue-ts`, `svelte-ts`, `solid-ts`, `vanilla-ts`) | React templates: Oxlint, TypeScript 7 and a solution-style tsconfig; `--eslint` switches to ESLint; no test runner | Vitest options go in `vite.config.*` ([test](toolchain-lint-test.md#test)) | `vitejs/vite/packages/create-vite/README.md`; flags: `vitejs/vite/packages/create-vite/src/index.ts` |
| Vite+ | `npm create vite-plus@latest -- vite:application --directory <dir> --no-interactive --no-hooks --no-agent` | Vitest, Oxlint, Oxfmt, Rolldown; `vp check` runs format, lint and types | The first argument is the template, not the folder. Add `format:check`, `lint`, `typecheck` scripts that call `vp` so `verify-project.mjs` finds the gates | `voidzero-dev/vite-plus/docs/guide/create.md` |
| Astro 7 | `npm create astro@latest <dir> -- --template minimal --install --no-git --yes --skip-houston --no-ai` (`--template starlight` for docs, `--add react`) | Build scripts only | Writes AI agent files unless `--no-ai` | `withastro/astro/packages/create-astro/README.md` |

## Edge, runtimes and extensions

| Target | Non-interactive scaffold | Sets up | Watch | Docs |
|---|---|---|---|---|
| Cloudflare Workers | `npm create cloudflare@latest -- <dir> --type=hello-world --lang=ts --no-deploy --no-git --no-open -y` (`--framework <name>` for a framework app) | Prettier, `.editorconfig`, Vitest config and a test | Opens a browser unless `--no-open` | `cloudflare/workers-sdk/packages/create-cloudflare/src/helpers/args.ts` |
| Hono | The [services](stacks-services.md#apis-and-services) row, with `--template cloudflare-workers`, `bun` or `deno` | | | |
| Fresh 2 (Deno) | `deno run -Ar jsr:@fresh/init <dir> --tailwind --vscode` (`--force` for a non-empty folder, `--docker`) | Vite build; Deno's fmt, lint, check and test | Prompts for any omitted flag. The repository moved to freshframework/fresh | `freshframework/fresh/packages/init/src/init.ts` |
| Browser extension | `npx wxt@latest init <dir> -t react --pm <pm>` | — | Add the lint and test lanes | `wxt-dev/wxt/docs/guide/installation.md` |

## Monorepos

| Tool | Non-interactive scaffold | Sets up | Watch | Docs |
|---|---|---|---|---|
| Turborepo | `npx create-turbo@latest <dir> -m <pm> --skip-install` (`-e <example>`) | Prettier, a shared ESLint config package, `lint` and `check-types` tasks | The basic example needs Node.js 24+ | `vercel/turborepo/apps/docs/content/docs/reference/create-turbo.mdx` |
| Nx | `npx create-nx-workspace@latest <name> --template=empty --interactive=false --nxCloud=<choice>` (`--preset`, `--linter eslint` or `oxlint`, `--unitTestRunner`, `--aiAgents none`) | ESLint and Prettier by default | Pass `--nxCloud` explicitly | `nrwl/nx/packages/create-nx-workspace/bin/create-nx-workspace.ts` |

In a monorepo, run `verify-project.mjs` on each app or package folder, or pass the root task as `--gate test="pnpm turbo run test"`.

Superseded CLIs such as Create React App: [never scaffold with them](boilerplates.md#superseded-do-not-start-new-projects-with-these).
