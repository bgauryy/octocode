/**
 * Token resolution from environment variables.
 *
 * Priority is the canonical order in `ENV_TOKEN_VARS`. Callers can pass an
 * effective environment populated by propagateOctocodeEnv: explicit values
 * win across credential aliases, then workspace .env, then home .env.
 * Alias priority here breaks ties within the winning source.
 * These helpers do not load files or mutate the environment.
 */
import type { TokenSource } from './types.js';
import {
  ENV_TOKEN_VARS,
  type EnvTokenVar,
} from '../config/contract.generated.js';

export { ENV_TOKEN_VARS, type EnvTokenVar };

/** Return the first non-empty token value found in env, or null. */
export function getTokenFromEnv(env: NodeJS.ProcessEnv = process.env): string | null {
  for (const envVar of ENV_TOKEN_VARS) {
    const token = env[envVar];
    if (token && token.trim()) return token.trim();
  }
  return null;
}

/** Return the source label for the first non-empty token var, or null. */
export function getEnvTokenSource(env: NodeJS.ProcessEnv = process.env): TokenSource {
  for (const envVar of ENV_TOKEN_VARS) {
    const token = env[envVar];
    if (token && token.trim()) return `env:${envVar}`;
  }
  return null;
}

/** True when at least one token env var is set and non-empty. */
export function hasEnvToken(env: NodeJS.ProcessEnv = process.env): boolean {
  return getTokenFromEnv(env) !== null;
}

/** Return { token, source } for the first matching var, or null. */
export function resolveEnvToken(
  env: NodeJS.ProcessEnv = process.env,
): { token: string; source: Exclude<TokenSource, null | 'octocode-storage' | 'gh-cli'> } | null {
  for (const envVar of ENV_TOKEN_VARS) {
    const token = env[envVar];
    if (token?.trim()) {
      return {
        token: token.trim(),
        source: `env:${envVar}`,
      };
    }
  }
  return null;
}
