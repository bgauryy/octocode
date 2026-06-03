/**
 * Environment variable token lookup (zero dependencies on storage).
 *
 * Extracted to break the storage.ts ↔ tokenResolution.ts cycle.
 */

import type { TokenSource } from './types.js';

export const ENV_TOKEN_VARS = [
  'OCTOCODE_TOKEN',
  'GH_TOKEN',
  'GITHUB_TOKEN',
] as const;

export function getTokenFromEnv(): string | null {
  for (const envVar of ENV_TOKEN_VARS) {
    const token = process.env[envVar];
    if (token && token.trim()) {
      return token.trim();
    }
  }
  return null;
}

export function getEnvTokenSource(): TokenSource {
  for (const envVar of ENV_TOKEN_VARS) {
    const token = process.env[envVar];
    if (token && token.trim()) {
      return `env:${envVar}` as TokenSource;
    }
  }
  return null;
}

export function hasEnvToken(): boolean {
  return getTokenFromEnv() !== null;
}

/** Returns token + source in a single pass, avoiding the double scan. */
export function resolveEnvToken(): {
  token: string;
  source: NonNullable<TokenSource>;
} | null {
  for (const envVar of ENV_TOKEN_VARS) {
    const token = process.env[envVar];
    if (token?.trim()) {
      return {
        token: token.trim(),
        source: `env:${envVar}` as NonNullable<TokenSource>,
      };
    }
  }
  return null;
}
