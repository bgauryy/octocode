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
export { getOctocodeHome } from './home.js';
export {
  loadOctocodeEnv,
  applyOctocodeEnv,
  propagateOctocodeEnv,
} from './dotenv.js';
export { devOverridesAllowed } from './config/devOverrides.js';

/** Every setting's generated default (the native resolver's base layer). */
export const DEFAULT_CONFIG = Object.freeze(DEFAULT_CONFIG_VALUE);

/**
 * The env var bound to a config field (e.g. `classification.api`), or
 * `undefined` for an unknown path or an env-less field.
 */
export function configFieldEnvName(fieldPath: string): string | undefined {
  return CONFIG_FIELDS.find(candidate => candidate.path === fieldPath)?.env
    ?.name;
}
