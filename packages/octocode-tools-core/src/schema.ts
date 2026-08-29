/**
 * `@octocodeai/octocode-tools-core/schema` — the engine-FREE direct-tool schema
 * surface (P3). Re-exports only the metadata/schema-text/display/input-prep API
 * from `directToolCatalog.meta.ts`, with NO transitive `@octocodeai/octocode-engine`
 * import. Importing this never loads the native `.node` addon, so consumers that
 * only need `--scheme` / help / `context` (e.g. the CLI on engine-less runtimes
 * like Codex.app Node) can read schemas without the engine.
 *
 * For execution, import `executeDirectTool` from `@octocodeai/octocode-tools-core/direct`.
 */
export * from './tools/directToolCatalog.meta.js';
export * from './toolContract/schemas.js';
export * from './toolContract/runtime.js';
// Descriptions and executable schemas are repository-owned; the loaded content
// retains the external shared system prompt until that separate surface moves.
export { loadToolContent } from './tools/toolMetadata/state.js';
