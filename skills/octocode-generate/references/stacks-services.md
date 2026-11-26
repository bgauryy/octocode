# Service, CLI, library and AI stacks

Load when the project is an API, a server-rendered monolith, a CLI, a published library, an AI agent or an MCP server. Verify each row before use as [research](research.md) describes. Each stack's gates are in [toolchain](toolchain.md#gates-per-ecosystem).

## APIs and services

| Stack | Non-interactive scaffold | Sets up | Watch | Docs |
|---|---|---|---|---|
| Hono (TypeScript) | `npm create hono@latest <dir> -- --template nodejs --pm <pm> --install` (`bun`, `deno`, `cloudflare-workers`, …) | Minimal server | Add the `node-ts` and `ci-js` templates, a lint lane and Vitest | `honojs/create-hono/README.md` |
| Fastify 5 | `npx fastify-cli@latest generate <dir> --lang=ts` | `node --test` with c8 | Does not install; the folder must not exist unless it is `.` | `fastify/fastify-cli/README.md` |
| NestJS 12 | `npx @nestjs/cli@latest new <dir> --package-manager <pm> --skip-git --type esm --no-observe --skip-skills` | ESM, Vitest, Oxlint and Prettier, AGENTS.md | Needs Node.js 22.22.3+, 24.15+ or 26+; CommonJS projects get Jest | `nestjs/docs.nestjs.com/content/first-steps.md` |
| Bun | `bun init -y` (`--react`, `--react=tailwind`, `--react=shadcn`); from a template `bun create <name or user/repo> <dir>` | One runtime for the server, bundler, tests and SQL: `Bun.serve({ routes })` serves HTML imports and API routes, with built-in `sql`; `bun test`; `bun check` type-checks; `bun build --compile ./cli.ts --outfile <name>` makes one executable (`--target` for other platforms) | No linter: add a lint lane | `oven-sh/bun/docs/bundler/fullstack.mdx`, `oven-sh/bun/docs/compiler/index.mdx` |
| Deno 2 | `deno init <dir>` (`--serve`, `--lib`) | `deno fmt`, `lint`, `check`, `test` built in | CI installs with `deno ci` once `deno.lock` exists | `denoland/docs/runtime/reference/cli/init.md` |
| FastAPI | `uv init --no-package <dir>`, then `uv add "fastapi[standard]"`; run `uv run fastapi dev` | — | Quote the extra in zsh. Full-stack starter: fastapi/full-stack-fastapi-template ("Use this template") | `fastapi/fastapi/README.md`, astral-sh/uv `astral-sh/uv/docs/guides/integration/fastapi.md` |
| Go | `go mod init <module>`; routing with the standard `net/http` mux | gofmt, vet, `go test` | golangci-lint v2 moved formatters to `golangci-lint fmt` | `golangci/golangci-lint/pkg/commands/fmt.go` |
| Rust (axum) | `cargo new <dir>`, then `cargo add axum tokio --features tokio/full` | Rust gates; template `rust` | Loco (Rails-like): `cargo install loco`, then `loco new --name <app> --db sqlite --bg async --assets serverside`; the `loco-cli` crate is stale | `loco-rs/loco/README.md` |
| ASP.NET Core (.NET 10) | `dotnet new sln -n <Name>`, `dotnet new webapi -o src/<Name>.Api`, `dotnet new xunit -o tests/<Name>.Api.Tests`, `dotnet sln add src/<Name>.Api tests/<Name>.Api.Tests`, `dotnet add tests/<Name>.Api.Tests reference src/<Name>.Api` | `.slnx` solution, tests beside the app; template `dotnet` | A test project inside the app's folder is compiled into the app. .NET 11 is out: pin the SDK with `global.json`. Blazor: `dotnet new blazor` | `dotnet/docs/docs/core/tools/dotnet-new-sdk-templates.md` |
| Spring Boot 4.2 | `curl -G https://start.spring.io/starter.zip -d type=gradle-project-kotlin -d javaVersion=25 -d bootVersion=4.2.x -d dependencies=web,actuator -d artifactId=<name> -o <name>.zip`, then unzip into `<dir>` | Gradle wrapper and tests; no formatter or linter; template `gradle`, applied with `--java 25` | The defaults are a Groovy build and Java 17: pass both | https://start.spring.io/metadata/client (live defaults) |
| Ktor 3 | `ktor new <name>` (install: `brew install ktor`); web generator start.ktor.io | Gradle build and tests | The CLI calls a remote generator: it needs the network | `ktorio/ktor-cli/README.md` |

## Web monoliths

| Stack | Non-interactive scaffold | Sets up | Watch | Docs |
|---|---|---|---|---|
| Rails 8.1 | `gem install rails`, then `rails new <dir> -d postgresql` (default SQLite; `--skip-kamal`, `--skip-ci`) | RuboCop (omakase), Brakeman, bundler-audit, a GitHub CI workflow, `bin/ci` | Needs Ruby 3.2+ (macOS system Ruby is 2.6). The generated CI pins old action tags: re-pin them ([research](research.md#recipe), step 7) | `rails/rails/railties/lib/rails/generators/rails/app/templates/github/ci.yml.tt` |
| Laravel 13 | `composer global require laravel/installer`, then `laravel new <dir> --no-interaction --pest --database=sqlite --no-boost --no-node` (`--react`, `--vue`, `--svelte`, `--livewire` starter kits) | Pint, Pest | Needs PHP 8.3+. For the lint gate add Larastan: `composer require --dev larastan/larastan` | `laravel/installer/src/NewCommand.php` |
| Django 6 | `uv init --bare <dir>`, then `uv add django` and `uv run django-admin startproject <name> .` | `manage.py test` | Needs Python 3.12+. Add pytest + pytest-django, or pass `--gate test="uv run python manage.py test"` | `django/django/docs/intro/tutorial01.txt` |
| Phoenix 1.8 | `mix archive.install hex phx_new --force`, then `mix phx.new <app> --no-install` and `mix deps.get` | `mix precommit` alias (compile with warnings as errors, format, test) | Prompts unless `--install` or `--no-install`. Add Credo for the lint gate. Needs Elixir 1.17+ and Erlang 25+ | `phoenixframework/phoenix/installer/lib/mix/tasks/phx.new.ex` |

## CLIs and libraries

| Need | Scaffold | Watch | Docs |
|---|---|---|---|
| Rust CLI | `cargo new <dir>`, then `cargo add clap --features derive`; TUI: `cargo generate ratatui/templates simple --name <app>` | `cargo install cargo-generate` first | ratatui/templates |
| Go CLI | `go mod init <module>`, then `go get github.com/spf13/cobra` | The `cobra-cli` generator's last release was in 2022; the library is current | `spf13/cobra-cli/README.md` |
| Python CLI | `uv init --package <dir>`, then `uv add typer` | Run it with `uv run <name>` | `fastapi/typer/README.md` |
| TypeScript library | `npm create tsdown@latest <dir> -- -t default` (`minimal`, `react`, `vue`, `solid`, `svelte`) | Building needs Node.js 22+; check the package with `publint` and `@arethetypeswrong/cli` | `rolldown/tsdown/packages/create-tsdown/README.md` |
| Other libraries | `cargo new --lib <dir>`; `uv init --lib <dir>`; `swift package init --type library --name <Name> --enable-swift-testing`; `dotnet new classlib -o src/<Name>`; `go mod init` | Swift: run `swift format --in-place --recursive Sources Tests Package.swift` once | `swiftlang/swift-package-manager/Sources/PackageManagerDocs/Documentation.docc/Package/PackageInit.md` |

## AI apps, agents and MCP servers

| Need | Scaffold | Watch | Docs |
|---|---|---|---|
| AI chat or generation in a web app | Next.js row, then `<pm> add ai @ai-sdk/react zod` | The AI Gateway provider ships in `ai` (`AI_GATEWAY_API_KEY`); other providers are `@ai-sdk/<provider>` | `vercel/ai/content/docs/02-getting-started/02-nextjs-app-router.mdx` |
| Agents and workflows (TypeScript) | `npm create mastra@latest <dir> -- --llm <provider> --no-skills --no-git --no-install` | Sends analytics; installs coding-assistant skills unless `--no-skills` | `mastra-ai/mastra/packages/cli/src/commands/create/command.ts` |
| MCP server, TypeScript | A Node.js TypeScript project (`node-ts`, `ci-js`), then `<pm> add @modelcontextprotocol/server zod`; import `McpServer` and the stdio transport | SDK v2 ships as `@modelcontextprotocol/server` and `/client`; `@modelcontextprotocol/sdk` 1.x is the maintenance line; `@modelcontextprotocol/create-server` is deprecated | `modelcontextprotocol/typescript-sdk/README.md` |
| MCP server, Python | `uv init --package <dir>`, then `uv add "mcp[cli]"`; `from mcp.server import MCPServer`; test with `uv run mcp dev server.py`. Or `uv add fastmcp` | mcp 2.x removed `mcp.server.fastmcp` | `modelcontextprotocol/python-sdk/docs/migration.md` |
