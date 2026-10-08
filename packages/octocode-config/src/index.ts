// @octocodeai/config — Octocode home, `.env` policy, and config contract
// metadata for every TypeScript surface (CLI launcher, MCP server, VS Code,
// skills). Configuration itself is resolved by the native runtime from the same
// contract; this entry is zero-dependency (Node builtins only). The `./schema`
// and `./mcp` subpaths re-export `@octocodeai/octocode-core`.

import {
  CONFIG_FIELDS,
  DEFAULT_CONFIG_VALUE,
} from './config/contract.generated.js';

export { CONFIG_FIELDS, ENV_TOKEN_VARS } from './config/contract.generated.js';
export type { RuntimeSurface } from './config/contract.generated.js';
export {
  getOctocodeHome,
  getConfigFilePath,
  getProjectConfigFilePath,
} from './home.js';
export {
  loadOctocodeEnv,
  applyOctocodeEnv,
  propagateOctocodeEnv,
} from './dotenv.js';
export {
  devOverridesAllowed,
  contractDriftAllowed,
  contractDriftMessage,
} from './config/devOverrides.js';

/** Every setting's generated default (the native resolver's base layer). */
export const DEFAULT_CONFIG = Object.freeze(DEFAULT_CONFIG_VALUE);

/**
 * Per-request execution budget for interactive surfaces (CLI and MCP): cold
 * start plus one logical LSP request — initialize, Java readiness, retries,
 * delays, and transport overhead. The native CLI's
 * `INTERACTIVE_EXECUTION_TIMEOUT_SECS` uses the same value.
 */
export const INTERACTIVE_EXECUTION_TIMEOUT_SECS = 300;

/**
 * Env var names bound to a config field (e.g. `classification.api`), in the
 * generated priority order. Empty for an unknown path or an env-less field.
 */
export function configFieldEnvNames(fieldPath: string): readonly string[] {
  const field = CONFIG_FIELDS.find(candidate => candidate.path === fieldPath);
  return (field?.env ?? []).map(binding => binding.name);
}
