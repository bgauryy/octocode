/**
 * Direct-tool catalog barrel (P3). The catalog is split so the schema/help path
 * never loads the native engine:
 *   - `directToolCatalog.meta.ts` — engine-free definitions, schema text,
 *     display fields, input preparation (also exposed via the `/schema` subpath).
 *   - `directToolCatalog.exec.ts` — `executeDirectTool` + the engine/runtime.
 *
 * This barrel preserves the historical `@octocodeai/octocode-tools-core/direct`
 * surface (meta + executeDirectTool). Importing it still eagerly loads the engine
 * (via the exec module) — execution consumers want that. Pure schema/help/
 * `--scheme`/`context` callers must import from `/schema` (meta only) instead, so
 * they run on engine-less runtimes.
 */
export * from './directToolCatalog.meta.js';
export { executeDirectTool } from './directToolCatalog.exec.js';
