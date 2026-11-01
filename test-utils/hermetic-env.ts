/**
 * hermetic-env.ts — the one definition of a hermetic test environment, shared
 * by test-utils/external-effects-guard.ts (vitest) and
 * packages/octocode-benchmark/local/e2e/harness (Node imports this file by type stripping, so
 * it uses erasable TypeScript only).
 *
 * A hermetic environment has a fresh empty Octocode home and gh config dir, no
 * GitHub or classifier credentials, and HTTP(S) routed to a dead proxy with
 * loopback exempt, so local fixture servers stay reachable.
 */
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

/** Port 9 (discard) on loopback refuses at once. */
export const DEAD_PROXY = 'http://127.0.0.1:9';
export const LOOPBACK_NO_PROXY = 'localhost,127.0.0.1,::1';
/** Any `*TOKEN` the host shell carries (GitHub, CI, editor sessions) and classifier settings. */
export const CREDENTIAL_ENV = /TOKEN$|^OCTOCODE_CLASSIFICATION_/;
/** Flags that switch suites onto live services. */
export const LIVE_ENV = ['OCTOCODE_LIVE', 'RUN_MCP_LIVE', 'RUN_CHROME_LIVE', 'OCTOCODE_CHROME_DEBUG_E2E'];
const PROXY_ENV = ['HTTPS_PROXY', 'HTTP_PROXY', 'ALL_PROXY'];
const homes = new Set<string>();

function removeHomes(): void {
  for (const home of homes) rmSync(home, { recursive: true, force: true });
}

/**
 * `base` without credentials or live flags, with a new empty home under the OS
 * temp dir (`label` names it) and the dead proxy. The home is removed when the
 * process exits; vitest workers skip exit handlers, so the vitest guard removes its own.
 */
export function hermeticEnv(
  base: NodeJS.ProcessEnv = process.env,
  label = 'test'
): NodeJS.ProcessEnv {
  const home = mkdtempSync(join(tmpdir(), `octocode-${label}-home-`));
  if (homes.size === 0) process.once('exit', removeHomes);
  homes.add(home);
  const env: NodeJS.ProcessEnv = {};
  for (const [name, value] of Object.entries(base)) {
    if (!CREDENTIAL_ENV.test(name) && !LIVE_ENV.includes(name)) env[name] = value;
  }
  env['OCTOCODE_HOME'] = home;
  env['GH_CONFIG_DIR'] = join(home, 'gh');
  env['OCTOCODE_LSP_PREWARM'] = 'off';
  env['OCTOCODE_LSP_AUTO_INSTALL'] = 'off';
  env['OCTOCODE_TRUST_PROJECT_LSP_CONFIG'] = 'false';
  for (const name of PROXY_ENV) env[name] = env[name.toLowerCase()] = DEAD_PROXY;
  env['NO_PROXY'] = env['no_proxy'] = LOOPBACK_NO_PROXY;
  return env;
}
