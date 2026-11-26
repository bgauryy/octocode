Load when a setting resolves from an unexpected source, or when declaring an environment binding or dotenv policy.

# Configuration sources and environment bindings

Declaring the setting itself: [ADDING_CONFIG.md](ADDING_CONFIG.md). `crates/` is `packages/octocode-native/crates/`.

## Sources

Highest first, per field: process environment or MCP client `env` block → workspace `.octocode/.env` → home `.env` → workspace `.octocode/.octocoderc` → home `.octocoderc` → generated default. Invalid, missing or blank values fall through. A workspace `.octocoderc` field whose environment binding is protected (`dotenv` other than `all`) is ignored with a `workspace_config_protected` warning, like the workspace `.env`. CLI and MCP load both `.env` files; Node helpers load the supplied workspace unless `trusted:false`. Native LSP-project trust is separate.

## Environment binding

At most one `env` binding per setting, `{ "name": "OCTOCODE_..." }`. Every Octocode variable must be in the contract, or source labeling, protection, docs and the resolver cannot derive it. Variables other programs own (toolchain homes, `NPM_CONFIG_USERCONFIG`, `GIT_CONFIG_*`, `XDG_CONFIG_HOME`) are not settings; `PROCESS_ENV` in `crates/engine/src/process_env.rs` lists every one the engine reads so the warm server can key on them. An invalid environment value is skipped with a warning.

| Property | Effect |
|---|---|
| `dotenv` | `all` (default: workspace or home `.env`), `home` (the trusted home `.env` only), `never` (shell, CI or MCP environment only; joins the generated protected-key sets) |
| `normalize` | `"trim"`, or `"lower"` (trims and lowercases) |
| `workspaceNarrowsTo` | On a `dotenv: "home"` binding, the one value a workspace may still set (as `storage.mode: memory`) |
