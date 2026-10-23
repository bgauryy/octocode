/**
 * external-effects-guard.ts — Vitest setup file (setupFiles) shared by the
 * octocode-mcp, octocode and @octocodeai/config suites.
 *
 * Makes every unit test hermetic by construction:
 * - a fresh empty OCTOCODE_HOME and GH_CONFIG_DIR per test file, so no test
 *   reads the user's ~/.octocode (auth.json, .octocoderc, .env) or gh login;
 * - no GitHub token or classification key from the developer's shell;
 * - outbound HTTP(S) from native code goes to a dead proxy (127.0.0.1:9) and
 *   outbound `fetch` rejects; loopback stays reachable for local fixtures;
 * - child processes are limited to the current Node binary unless a test
 *   names the local executables it runs (e.g. the packaged native binary).
 *
 * Tests that need any of these must inject their own fake. The guard fails
 * loudly so the need is never silently skipped. The temp home lives under the
 * OS temp directory, which the OS reclaims.
 *
 * Contracts verified by packages/octocode-config/tests/external-effects-guard.test.ts.
 */
import { vi } from 'vitest';
import { hermeticEnv } from './hermetic-env.js';

// ─── Hermetic env: home, credentials, live flags, network sink ──────────────────────
// Node's process.env setters update the real environment, so the native binary
// and spawned children (reqwest system-proxy) see the same isolated env.
// Deleting (not blanking, as vitest's `env` would) keeps flags undefined.
const hermetic = hermeticEnv(process.env, 'test');
for (const name of Object.keys(process.env)) {
  if (!(name in hermetic)) delete process.env[name];
}
Object.assign(process.env, hermetic);
export const HERMETIC_HOME = hermetic['OCTOCODE_HOME']!;

const BLOCK_PROCESS = 'TEST_EXTERNAL_EFFECT_BLOCKED: inject a process/browser mock';
const BLOCK_FETCH = 'TEST_EXTERNAL_EFFECT_BLOCKED: inject a fetch mock';

/** Absolute paths of local executables a test file opted into. */
const allowedExecutables = new Set<string>();

/**
 * Permit the named local executables (absolute paths, e.g. the packaged
 * native binary) for the calling test file. The returned cleanup revokes them.
 */
export function allowLocalExecutables(...paths: string[]): () => void {
  for (const path of paths) allowedExecutables.add(path);
  return () => {
    for (const path of paths) allowedExecutables.delete(path);
  };
}

// ─── Fetch guard ─────────────────────────────────────────────────────────────
// Loopback fixtures stay reachable (tests exercise real HTTP framing against
// ephemeral local servers); every outbound request rejects.
const realFetch = globalThis.fetch.bind(globalThis);

function isLoopback(url: URL): boolean {
  return (
    (url.protocol === 'http:' || url.protocol === 'https:') &&
    (url.hostname === 'localhost' || url.hostname === '127.0.0.1' || url.hostname === '[::1]')
  );
}

globalThis.fetch = ((input: Parameters<typeof fetch>[0], init?: RequestInit): Promise<Response> => {
  const url =
    typeof input === 'string' ? input : input instanceof URL ? input.href : (input as Request).url;
  if (/^https?:\/\//i.test(url) && isLoopback(new URL(url))) return realFetch(input, init);
  return Promise.reject(new Error(`${BLOCK_FETCH}: ${url}`));
}) as typeof fetch;

// ─── Child-process guard ─────────────────────────────────────────────────────
// Only the current Node.js binary (the node launcher and deterministic Node
// sub-processes) and the executables a test allowed. A test file that mocks
// node:child_process itself replaces this guard with its own fake.
vi.mock('node:child_process', async importOriginal => {
  const cp = await importOriginal<typeof import('node:child_process')>();

  function isSafe(cmd: string): boolean {
    if (cmd === process.execPath || allowedExecutables.has(cmd)) return true;
    // Shell-style commands whose first token is the Node.js executable
    // (possibly double-quoted), e.g. `"${process.execPath}" "script.mjs"`.
    const firstToken = cmd.startsWith('"')
      ? cmd.slice(1, cmd.indexOf('"', 1))
      : (cmd.split(/\s/)[0] ?? '');
    return firstToken === process.execPath;
  }

  const guard =
    <F extends (...args: never[]) => unknown>(original: F): F =>
    ((cmd: string, ...rest: unknown[]) => {
      if (!isSafe(cmd)) throw new Error(`${BLOCK_PROCESS}: ${cmd}`);
      return (original as unknown as (...a: unknown[]) => unknown)(cmd, ...rest);
    }) as unknown as F;

  return {
    ...cp,
    spawn: guard(cp.spawn),
    spawnSync: guard(cp.spawnSync),
    execFile: guard(cp.execFile),
    execFileSync: guard(cp.execFileSync),
    exec: guard(cp.exec),
    execSync: guard(cp.execSync),
  };
});
