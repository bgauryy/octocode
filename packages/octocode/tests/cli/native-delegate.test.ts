import { describe, it, expect } from 'vitest';
import { chmodSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  resolveNativeBin,
  delegateToNative,
  NativeLaunchError,
} from '../../src/cli/native-delegate.js';

function makeFakeBin(body: string): string {
  const dir = mkdtempSync(join(tmpdir(), 'native-bin-'));
  const bin = join(dir, 'octocode');
  writeFileSync(bin, `#!/usr/bin/env node\n${body}\n`);
  chmodSync(bin, 0o755);
  return bin;
}

describe('resolveNativeBin', () => {
  it('resolves the platform binary directly (no launcher shim) or says why not', async () => {
    // Environment-agnostic: with no OCTOCODE_NATIVE_BIN the result is either the
    // compiled platform binary (monorepo/installed) or the resolver's own
    // error text — never a throw, never a `.cjs` launcher.
    const resolved = await resolveNativeBin({});
    if ('error' in resolved) {
      expect(resolved.error.length).toBeGreaterThan(0);
    } else {
      expect(resolved.bin).toMatch(/[\\/]octocode(\.exe)?$/);
      expect(resolved.bin.endsWith('.cjs')).toBe(false);
    }
  });

  it('names the missing path when OCTOCODE_NATIVE_BIN points nowhere', async () => {
    const resolved = await resolveNativeBin({
      OCTOCODE_NATIVE_BIN: '/no/such/octocode',
    });
    expect(resolved).toEqual({
      error: expect.stringContaining('OCTOCODE_NATIVE_BIN'),
    });
    expect('error' in resolved && resolved.error).toContain(
      '/no/such/octocode'
    );
  });

  it('keeps the resolver error, with its install fix, instead of a generic line', async () => {
    const platform = Object.getOwnPropertyDescriptor(process, 'platform')!;
    Object.defineProperty(process, 'platform', { value: 'plan9' });
    try {
      const resolved = await resolveNativeBin({});
      expect(resolved).toEqual({
        error: expect.stringContaining("unsupported platform 'plan9-"),
      });
    } finally {
      Object.defineProperty(process, 'platform', platform);
    }
  });

  it('returns the explicit path when it exists', async () => {
    const bin = makeFakeBin('process.exit(0)');
    await expect(
      resolveNativeBin({ OCTOCODE_NATIVE_BIN: bin })
    ).resolves.toEqual({ bin });
  });

  it('ignores OCTOCODE_NATIVE_BIN in production', async () => {
    const bin = makeFakeBin('process.exit(0)');
    const fallback = await resolveNativeBin({});
    await expect(
      resolveNativeBin({ OCTOCODE_NATIVE_BIN: bin, NODE_ENV: 'production' })
    ).resolves.toEqual(fallback);
    await expect(
      resolveNativeBin({
        OCTOCODE_NATIVE_BIN: '/no/such/octocode',
        NODE_ENV: 'production',
      })
    ).resolves.toEqual(fallback);
  });
});

describe('delegateToNative', () => {
  it('passes through the child exit code (success)', async () => {
    await expect(
      delegateToNative(process.execPath, ['-e', 'process.exit(0)'])
    ).resolves.toBe(0);
  });

  it('passes through a non-zero exit code', async () => {
    await expect(
      delegateToNative(process.execPath, ['-e', 'process.exit(7)'])
    ).resolves.toBe(7);
  });

  it('fails with the bin and the cause when the binary cannot be spawned', async () => {
    const failure = await delegateToNative(
      '/no/such/octocode-binary',
      []
    ).catch((error: unknown) => error);
    // Not exit 1 ("empty result") and not silent: a typed launch error.
    expect(failure).toBeInstanceOf(NativeLaunchError);
    const { message } = failure as NativeLaunchError;
    expect(message).toContain('/no/such/octocode-binary');
    expect(message).toMatch(/ENOENT/);
  });

  it.skipIf(process.platform === 'win32')(
    'reports a binary that is not executable',
    async () => {
      const bin = makeFakeBin('process.exit(0)');
      chmodSync(bin, 0o644);
      await expect(delegateToNative(bin, [])).rejects.toThrow(/EACCES/);
    }
  );

  it('preserves argument boundaries, environment, and working directory', async () => {
    const args = ['value with spaces', '--literal=equals=value', 'Unicode: λ'];
    const script = `
      const expected = JSON.parse(process.env.OCTOCODE_TEST_EXPECTED);
      const valid = JSON.stringify(process.argv.slice(1)) === JSON.stringify(expected.args)
        && process.cwd() === expected.cwd;
      process.exit(valid ? 0 : 9);
    `;
    await expect(
      delegateToNative(process.execPath, ['-e', script, '--', ...args], {
        ...process.env,
        OCTOCODE_TEST_EXPECTED: JSON.stringify({ args, cwd: process.cwd() }),
      })
    ).resolves.toBe(0);
  });
});
