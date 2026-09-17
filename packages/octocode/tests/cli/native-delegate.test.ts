import { describe, it, expect } from 'vitest';
import { chmodSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  TS_ONLY_COMMANDS,
  resolveNativeBin,
  shouldDelegateToNative,
  delegateToNative,
} from '../../src/cli/native-delegate.js';

function makeFakeBin(body: string): string {
  const dir = mkdtempSync(join(tmpdir(), 'native-bin-'));
  const bin = join(dir, 'octocode');
  writeFileSync(bin, `#!/usr/bin/env node\n${body}\n`);
  chmodSync(bin, 0o755);
  return bin;
}

describe('resolveNativeBin', () => {
  it('resolves the launcher shim or null when nothing is explicitly configured', () => {
    // Environment-agnostic: with no OCTOCODE_NATIVE_BIN the result is either the
    // resolvable `@octocodeai/octocode-native` launcher (monorepo/installed) or
    // null (not installed) — never a throw and never an arbitrary path.
    const resolved = resolveNativeBin({});
    expect(resolved === null || /octocode\.cjs$/.test(resolved)).toBe(true);
  });

  it('returns null when OCTOCODE_NATIVE_BIN points at a missing path', () => {
    expect(
      resolveNativeBin({ OCTOCODE_NATIVE_BIN: '/no/such/octocode' })
    ).toBeNull();
  });

  it('returns the explicit path when it exists', () => {
    const bin = makeFakeBin('process.exit(0)');
    expect(resolveNativeBin({ OCTOCODE_NATIVE_BIN: bin })).toBe(bin);
  });
});

describe('shouldDelegateToNative', () => {
  const bin = makeFakeBin('process.exit(0)');
  const nativeEnv = { OCTOCODE_RUNTIME: 'native', OCTOCODE_NATIVE_BIN: bin };

  it('does not delegate without the opt-in flag', () => {
    expect(shouldDelegateToNative('search', { OCTOCODE_NATIVE_BIN: bin })).toBe(
      false
    );
  });

  it('does not delegate with no command', () => {
    expect(shouldDelegateToNative(undefined, nativeEnv)).toBe(false);
  });

  it('does not delegate TS-only commands', () => {
    for (const command of TS_ONLY_COMMANDS) {
      expect(shouldDelegateToNative(command, nativeEnv)).toBe(false);
    }
  });

  it('does not delegate when opted in but the explicit binary is missing', () => {
    expect(
      shouldDelegateToNative('search', {
        OCTOCODE_RUNTIME: 'native',
        OCTOCODE_NATIVE_BIN: '/no/such/octocode',
      })
    ).toBe(false);
  });

  it('delegates a covered command when opted in and binary resolves', () => {
    expect(shouldDelegateToNative('ast', nativeEnv)).toBe(true);
    expect(shouldDelegateToNative('graph', nativeEnv)).toBe(true);
    expect(shouldDelegateToNative('tools', nativeEnv)).toBe(true);
  });
});

describe('delegateToNative', () => {
  it('passes through the child exit code (success)', () => {
    const bin = makeFakeBin('process.exit(0)');
    expect(delegateToNative(bin, ['--help'])).toBe(0);
  });

  it('passes through a non-zero exit code', () => {
    const bin = makeFakeBin('process.exit(7)');
    expect(delegateToNative(bin, [])).toBe(7);
  });

  it('returns 1 when the binary cannot be spawned', () => {
    expect(delegateToNative('/no/such/octocode-binary', [])).toBe(1);
  });

  it('runs a .cjs launcher via the node executable', () => {
    const dir = mkdtempSync(join(tmpdir(), 'native-launcher-'));
    const launcher = join(dir, 'octocode.cjs');
    writeFileSync(launcher, 'process.exit(3)\n');
    expect(delegateToNative(launcher, [])).toBe(3);
  });
});
