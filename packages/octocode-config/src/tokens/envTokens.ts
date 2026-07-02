/**
 * Token resolution from environment variables.
 *
 * Priority order (highest → lowest):
 *   OCTOCODE_TOKEN → GH_TOKEN → GITHUB_TOKEN → GITHUB_PERSONAL_ACCESS_TOKEN
 *
 * All four vars are in PROTECTED_KEYS and must never come from .env files.
 * They are read directly from process.env (set by the shell or MCP client).
 */
import type { TokenSource } from './types.js';

export const ENV_TOKEN_VARS = [
  'OCTOCODE_TOKEN',
  'GH_TOKEN',
  'GITHUB_TOKEN',
  'GITHUB_PERSONAL_ACCESS_TOKEN',
] as const;

export type EnvTokenVar = (typeof ENV_TOKEN_VARS)[number];

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
    if (token && token.trim()) return `env:${envVar}` as TokenSource;
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
        source: `env:${envVar}` as Exclude<TokenSource, null | 'octocode-storage' | 'gh-cli'>,
      };
    }
  }
  return null;
}
