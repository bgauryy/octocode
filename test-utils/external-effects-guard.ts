/**
 * external-effects-guard.ts — Vitest global setup
 *
 * Blocks accidental outbound network calls and external process launches
 * (browsers, system openers, remote npx packages) from unit tests.
 *
 * Tests that legitimately need real network or processes must inject their own
 * mocks. This guard fails loudly so the need is never silently skipped.
 *
 * Contracts verified by tests/external-effects-guard.test.ts.
 */
import { vi } from 'vitest';

// ─── Live-integration flag guard ──────────────────────────────────────────────────
// Vitest's env config can only set vars to '' (not delete them). Delete here so
// tests see undefined rather than an empty string for these integration flags.
delete process.env['OCTOCODE_CHROME_DEBUG_E2E'];
delete process.env['RUN_CHROME_LIVE'];
delete process.env['RUN_MCP_LIVE'];

const BLOCK_PROCESS = 'TEST_EXTERNAL_EFFECT_BLOCKED: inject a process/browser mock';
const BLOCK_FETCH = 'TEST_EXTERNAL_EFFECT_BLOCKED: inject a fetch mock';
const LOCAL_PROCESS_OPT_IN = 'OCTOCODE_TEST_ALLOW_LOCAL_PROCESS';

delete process.env[LOCAL_PROCESS_OPT_IN];

/**
 * Permit only the local fixture executables used by true integration tests.
 * The returned cleanup keeps the permission explicit and scoped to the file.
 */
export function allowLocalFixtureProcesses(): () => void {
  process.env[LOCAL_PROCESS_OPT_IN] = '1';
  return () => {
    delete process.env[LOCAL_PROCESS_OPT_IN];
  };
}

// ─── Fetch guard ─────────────────────────────────────────────────────────────
// Preserve hermetic loopback fixtures while rejecting every outbound request.
// Integration tests exercise real HTTP framing against ephemeral local servers;
// treating those requests as external effects made the guard test implementation
// details instead of the actual network boundary.
const realFetch = globalThis.fetch.bind(globalThis);

function isLoopback(url: URL): boolean {
  return (
    (url.protocol === 'http:' || url.protocol === 'https:') &&
    (url.hostname === 'localhost' || url.hostname === '127.0.0.1' || url.hostname === '[::1]')
  );
}

globalThis.fetch = ((input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
  const url =
    typeof input === 'string'
      ? input
      : input instanceof URL
        ? input.href
        : (input as Request).url;
  if (/^https?:\/\//i.test(url)) {
    const parsed = new URL(url);
    if (isLoopback(parsed)) return realFetch(input, init);
    return Promise.reject(new Error(BLOCK_FETCH));
  }
  return Promise.reject(new Error(`fetch called with unexpected input in tests: ${url}`));
}) as typeof fetch;

// ─── Child-process guard ─────────────────────────────────────────────────────
// Allow only the current Node.js binary (process.execPath) so tests can spawn
// deterministic Node sub-processes. Every other command is blocked.
vi.mock('node:child_process', async (importOriginal) => {
  const cp = await importOriginal<typeof import('node:child_process')>();

  function isSafe(cmd: string): boolean {
    if (cmd === process.execPath) return true;
    if (process.env[LOCAL_PROCESS_OPT_IN] !== '1') return false;
    return /(?:^|\/)(?:git|bash|zsh|sh)$/.test(cmd);
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const guardedSpawn = (cmd: string, ...rest: any[]): any => {
    if (!isSafe(cmd)) throw new Error(BLOCK_PROCESS);
    return (cp.spawn as (...a: unknown[]) => unknown)(cmd, ...rest);
  };

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const guardedSpawnSync = (cmd: string, ...rest: any[]): any => {
    if (!isSafe(cmd)) throw new Error(BLOCK_PROCESS);
    return (cp.spawnSync as (...a: unknown[]) => unknown)(cmd, ...rest);
  };

  return {
    ...cp,
    spawn: guardedSpawn,
    spawnSync: guardedSpawnSync,
  };
});
