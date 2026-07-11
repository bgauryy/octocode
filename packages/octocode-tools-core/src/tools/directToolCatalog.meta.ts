/**
 * Engine-free direct-tool catalog metadata (P3).
 *
 * This module holds EVERYTHING the schema/help/`--scheme`/`context` paths need
 * — tool definitions (name + display/bulk zod schemas), schema-text formatters,
 * display-field extraction, example builders, and input preparation — WITHOUT
 * importing `@octocodeai/octocode-engine` (no native `.node` load at module
 * eval). The schemas are sourced from each tool's engine-free `scheme.ts`, the
 * same modules `toolConfig.ts` consumes, so the two cannot drift on shape (a
 * drift test asserts name/schema parity against the runtime `ALL_TOOLS`).
 *
 * The execution path (`executeDirectTool`) lives in `directToolCatalog.exec.ts`,
 * which DOES import the engine; it is only reached when a tool actually runs.
 * The `@octocodeai/octocode-tools-core/schema` subpath re-exports only this
 * module so engine-less runtimes (e.g. Codex.app Node) can read schemas.
 *
 * This file is a thin barrel — the implementation is split across sibling
 * modules under `./directToolCatalog/` (registry/helpers, schema
 * introspection, command-pattern construction, and input preparation) purely
 * to keep each file under the repo's max-lines lint budget. No consumer
 * should need to import those sibling modules directly; import from here.
 */
export * from './directToolCatalog/toolCatalogDefinitions.js';
export * from './directToolCatalog/toolSchemaIntrospection.js';
export * from './directToolCatalog/toolCommandPatterns.js';
export * from './directToolCatalog/toolInputPreparation.js';
