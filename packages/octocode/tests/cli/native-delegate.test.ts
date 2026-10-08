import { describe, it, expect } from 'vitest';
import { chmodSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import {
  NODE_OWNED_COMMANDS,
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
  it('resolves the platform binary directly (no launcher shim) or null', () => {
    // Environment-agnostic: with no OCTOCODE_NATIVE_BIN the result is either the
    // compiled platform binary (monorepo/installed) or null (not installed) —
    // never a throw, never a `.cjs` launcher, never an arbitrary path.
    const resolved = resolveNativeBin({});
    expect(
      resolved === null ||
        (/[\\/]octocode(\.exe)?$/.test(resolved) && !resolved.endsWith('.cjs'))
    ).toBe(true);
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

  it('ignores OCTOCODE_NATIVE_BIN in production, like MCP OCTOCODE_NATIVE_BINDING', () => {
    const bin = makeFakeBin('process.exit(0)');
    expect(
      resolveNativeBin({ OCTOCODE_NATIVE_BIN: bin, NODE_ENV: 'production' })
    ).toBe(resolveNativeBin({}));
    expect(
      resolveNativeBin({
        OCTOCODE_NATIVE_BIN: '/no/such/octocode',
        NODE_ENV: 'production',
      })
    ).toBe(resolveNativeBin({}));
  });
});

describe('shouldDelegateToNative', () => {
  it('delegates covered commands without an opt-in flag', () => {
    expect(shouldDelegateToNative('search')).toBe(true);
  });

  it('delegates top-level help when no command is present', () => {
    expect(shouldDelegateToNative(undefined)).toBe(true);
  });

  it('does not delegate Node-owned commands', () => {
    for (const command of NODE_OWNED_COMMANDS) {
      expect(shouldDelegateToNative(command)).toBe(false);
    }
  });

  it('delegates every other command', () => {
    expect(shouldDelegateToNative('ast')).toBe(true);
    expect(shouldDelegateToNative('graph')).toBe(true);
    expect(shouldDelegateToNative('tools')).toBe(true);
  });

  it('delegates lsp-server', () => {
    expect(shouldDelegateToNative('lsp-server')).toBe(true);
  });

  it('delegates flag-only install', () => {
    // Interactive install is selected in index.ts from the absence of --ide.
    expect(shouldDelegateToNative('install')).toBe(true);
  });

  it('keeps skill materialization in Node to avoid native re-entry', () => {
    expect(shouldDelegateToNative('skill')).toBe(false);
  });
});

describe('delegateToNative', () => {
  it('passes through the child exit code (success)', async () => {
    const bin = makeFakeBin('process.exit(0)');
    await expect(delegateToNative(bin, ['--help'])).resolves.toBe(0);
  });

  it('passes through a non-zero exit code', async () => {
    const bin = makeFakeBin('process.exit(7)');
    await expect(delegateToNative(bin, [])).resolves.toBe(7);
  });

  it('returns 1 when the binary cannot be spawned', async () => {
    await expect(
      delegateToNative('/no/such/octocode-binary', [])
    ).resolves.toBe(1);
  });

  it('runs a .cjs launcher via the node executable', async () => {
    const dir = mkdtempSync(join(tmpdir(), 'native-launcher-'));
    const launcher = join(dir, 'octocode.cjs');
    writeFileSync(launcher, 'process.exit(3)\n');
    await expect(delegateToNative(launcher, [])).resolves.toBe(3);
  });
});
