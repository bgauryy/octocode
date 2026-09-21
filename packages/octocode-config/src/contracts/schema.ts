// In-repo hub for the tool-contract schema surface. The authoritative source
// stays in @octocodeai/octocode-core; octocode-config re-exports it so every
// Octocode surface (native regen, MCP, CLI, extensions) imports contracts from
// one place — "@octocodeai/config/schema" — instead of reaching into core.
export * from "@octocodeai/octocode-core/schema";
