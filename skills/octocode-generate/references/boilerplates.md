# Creational CLIs and boilerplates

Load when choosing how to create the project's first files. Prefer, in order:

1. The framework's own creational CLI: its row in the [web](stacks-web.md), [apps](stacks-apps.md) or [services](stacks-services.md) catalog.
2. A maintained boilerplate that already holds more of the features: auth, payments, dashboard, mobile plus web.
3. The skill's templates and hand-written files, only for what the first two leave out.

The [boilerplate catalog](boilerplate-catalog.md) lists the leading boilerplates per kind of project and how to judge one: read it when option 2 may apply.

## Ways to create

| Form | Command | Note |
|---|---|---|
| A `create-<name>` package | `npm create <name>@latest <dir> -- <flags>`, `pnpm create <name>`, `yarn create <name>`, `bun create <name> <dir>` | `bun create <name>` runs `create-<name>`; npm needs `--` before flags |
| A framework example | `npx create-next-app@latest --example <example-name or github-url> <dir>` (also `bun create next-app --example …`) | Any public GitHub example works |
| A Turborepo example | `npx create-turbo@latest -e <example-name or github-url> <dir>` | |
| Any GitHub boilerplate | `bun create <user>/<repo> <dir>`, or `git clone --depth 1 <url> <dir>`, then remove `<dir>/.git` | `bun create` runs the template's install hooks unless `--no-install`: read them first |
| One React component as an app | `bun create ./App.tsx` | Adds Tailwind and shadcn/ui when the component uses them; Bun calls it the Create React App successor |
| GitHub "Use this template" | Creates a repository on the user's account | An outward effect: ask first |

Read a boilerplate's install scripts before running them: they execute on install.

## Superseded: do not start new projects with these

| Instead of | Use |
|---|---|
| `create-react-app` (deprecated: react.dev, 2025-02-14) | Vite (`react-ts`), a framework, or `bun create ./App.tsx` |
| `create-remix` | `create-react-router` (Remix merged into React Router) |
| `create-svelte` | `sv create` |
| `create-tsrouter-app`, `create-start-app` (aliases the TanStack CLI still publishes) | `@tanstack/cli create`, the command TanStack's docs give |
| `@modelcontextprotocol/create-server` | The MCP SDK v2 packages |
| `loco-cli` | `cargo install loco` |
| `create-llama` (run-llama/create-llama is archived; last release 0.6.3, 2025-07-15) | The project names no successor: follow LlamaIndex's current docs |
| Electron Forge `--template=vite-typescript` (removed in Forge 8) | `--template=vite --typescript` |
