import type { PiInstance } from '../types.js';

export type GitHubAuthSource = 'octocode' | 'env' | 'gh-cli' | 'unknown';

export type GitHubAuthState =
  | { status: 'checking' }
  | { status: 'authenticated'; source: GitHubAuthSource }
  | { status: 'missing' }
  | { status: 'error' };

interface AuthStatusPayload {
  authenticated?: unknown;
  tokenPresent?: unknown;
  tokenSource?: unknown;
  tokenExpired?: unknown;
}

const AUTH_PROBE_TIMEOUT_MS = 10_000;

function normalizeSource(value: unknown): GitHubAuthSource {
  return value === 'octocode' || value === 'env' || value === 'gh-cli' ? value : 'unknown';
}

function parsePayload(stdout: string): AuthStatusPayload | undefined {
  const candidates = [stdout.trim(), ...stdout.trim().split(/\r?\n/).reverse()];
  for (const candidate of candidates) {
    if (!candidate.startsWith('{')) continue;
    try {
      const value = JSON.parse(candidate) as unknown;
      if (value && typeof value === 'object' && !Array.isArray(value)) {
        return value as AuthStatusPayload;
      }
    } catch {
      // Try a later candidate; npx may prepend informational output.
    }
  }
  return undefined;
}

/** Parse only the allowlisted, non-secret fields emitted by Octocode auth status. */
export function parseGitHubAuthStatus(stdout: string): GitHubAuthState {
  const payload = parsePayload(stdout);
  if (!payload) return { status: 'error' };

  if (payload.authenticated === true && payload.tokenExpired !== true) {
    return { status: 'authenticated', source: normalizeSource(payload.tokenSource) };
  }
  if (
    payload.authenticated === false
    || payload.tokenPresent === false
    || payload.tokenExpired === true
  ) {
    return { status: 'missing' };
  }
  return { status: 'error' };
}

/** Run Octocode's credential resolver without ever reading or returning a token. */
export async function probeGitHubAuth(
  exec: PiInstance['exec'],
): Promise<GitHubAuthState> {
  if (!exec) return { status: 'error' };
  try {
    const result = await exec(
      'npx',
      ['octocode', 'auth', 'status', '--json'],
      { timeout: AUTH_PROBE_TIMEOUT_MS },
    );
    const state = parseGitHubAuthStatus(result.stdout);
    return state.status !== 'error' || result.code === 0 ? state : { status: 'error' };
  } catch {
    return { status: 'error' };
  }
}
