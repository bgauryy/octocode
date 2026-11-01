// In-repo hub for the tool-contract schema surface. Zod schemas are authored in
// @octocodeai/octocode-core and re-exported here, so every Octocode surface
// imports contracts from "@octocodeai/config/schema". Native's Rust types are
// generated from the same contract (scripts/generate-tool-contract.ts).
export * from "@octocodeai/octocode-core/schema";
