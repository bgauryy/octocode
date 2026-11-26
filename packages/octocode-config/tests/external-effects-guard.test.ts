import { spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { tmpdir } from 'node:os';
import { describe, expect, it } from 'vitest';
import { getOctocodeHome } from '../src/home.js';
import {
  HERMETIC_HOME,
  allowLocalExecutables,
} from '../../../test-utils/external-effects-guard.js';
import { DEAD_PROXY } from '../../../test-utils/hermetic-env.js';

// test-utils/external-effects-guard.ts runs before every test file (setupFiles).
describe('external effects guard', () => {
  it('points the Octocode home at a fresh temp dir', () => {
    expect(HERMETIC_HOME.startsWith(tmpdir())).toBe(true);
    expect(getOctocodeHome()).toBe(HERMETIC_HOME);
  });

  it('leaves no credential in the environment', () => {
    const leaked = Object.keys(process.env).filter(name =>
      /TOKEN$|^(OCTOCODE_)?CLASSIFICATION_/.test(name)
    );
    expect(leaked).toEqual([]);
  });

  it.each(['HTTPS_PROXY', 'HTTP_PROXY', 'ALL_PROXY'])(
    'routes native %s traffic to the dead proxy',
    name => {
      expect(process.env[name]).toBe(DEAD_PROXY);
      expect(process.env[name.toLowerCase()]).toBe(DEAD_PROXY);
    }
  );

  it('rejects outbound fetch and names the URL', async () => {
    await expect(fetch('https://example.invalid/x')).rejects.toThrow(
      /TEST_EXTERNAL_EFFECT_BLOCKED.*example\.invalid/
    );
  });

  it('allows loopback fetch for local fixtures', async () => {
    const server = createServer((_, res) => res.end('ok'));
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    try {
      const { port } = server.address() as AddressInfo;
      const res = await fetch(`http://127.0.0.1:${port}/`);
      expect(await res.text()).toBe('ok');
    } finally {
      await new Promise(resolve => server.close(resolve));
    }
  });

  it('runs Node sub-processes and blocks any other executable', () => {
    expect(spawnSync(process.execPath, ['-e', '']).status).toBe(0);
    expect(() => spawnSync('git', ['--version'])).toThrow(
      /TEST_EXTERNAL_EFFECT_BLOCKED.*git/
    );
  });

  it.skipIf(process.platform === 'win32')(
    'runs an allowed local executable until the allowance is revoked',
    () => {
      const revoke = allowLocalExecutables('/usr/bin/true');
      expect(spawnSync('/usr/bin/true').status).toBe(0);
      revoke();
      expect(() => spawnSync('/usr/bin/true')).toThrow(
        /TEST_EXTERNAL_EFFECT_BLOCKED/
      );
    }
  );
});
