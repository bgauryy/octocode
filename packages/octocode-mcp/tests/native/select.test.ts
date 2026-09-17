import { describe, it, expect, vi } from 'vitest';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  resolveNativeAddon,
  selectRuntime,
  bootRuntime,
} from '../../src/native/select.js';

const here = dirname(fileURLToPath(import.meta.url));
const fakeAddon = join(here, 'fixtures/fake-addon.cjs');

describe('resolveNativeAddon', () => {
  it('returns null when OCTOCODE_NATIVE_BINDING is unset', () => {
    expect(resolveNativeAddon({})).toBeNull();
  });

  it('returns null when the binding cannot be required', () => {
    expect(
      resolveNativeAddon({ OCTOCODE_NATIVE_BINDING: '/no/such/addon.cjs' })
    ).toBeNull();
  });

  it('returns the path when the addon exports NativeRuntime', () => {
    expect(resolveNativeAddon({ OCTOCODE_NATIVE_BINDING: fakeAddon })).toBe(
      fakeAddon
    );
  });
});

describe('selectRuntime', () => {
  it('defaults to tools-core', () => {
    expect(selectRuntime({})).toBe('tools-core');
  });

  it('stays tools-core when opted in but no addon resolves', () => {
    expect(selectRuntime({ OCTOCODE_RUNTIME: 'native' })).toBe('tools-core');
  });

  it('stays tools-core when an addon exists but opt-in is absent', () => {
    expect(selectRuntime({ OCTOCODE_NATIVE_BINDING: fakeAddon })).toBe(
      'tools-core'
    );
  });

  it('selects native when opted in and an addon resolves', () => {
    expect(
      selectRuntime({
        OCTOCODE_RUNTIME: 'native',
        OCTOCODE_NATIVE_BINDING: fakeAddon,
      })
    ).toBe('native');
  });

  it('is case- and whitespace-insensitive for the opt-in flag', () => {
    expect(
      selectRuntime({
        OCTOCODE_RUNTIME: '  Native ',
        OCTOCODE_NATIVE_BINDING: fakeAddon,
      })
    ).toBe('native');
  });
});

describe('bootRuntime', () => {
  const nativeEnv = {
    OCTOCODE_RUNTIME: 'native',
    OCTOCODE_NATIVE_BINDING: fakeAddon,
  };

  it('boots tools-core by default and never calls native', async () => {
    const startNative = vi.fn(async () => {});
    const startToolsCore = vi.fn(async () => {});
    const kind = await bootRuntime({ env: {}, startNative, startToolsCore });
    expect(kind).toBe('tools-core');
    expect(startNative).not.toHaveBeenCalled();
    expect(startToolsCore).toHaveBeenCalledOnce();
  });

  it('boots native when selected and native start succeeds', async () => {
    const startNative = vi.fn(async () => {});
    const startToolsCore = vi.fn(async () => {});
    const kind = await bootRuntime({
      env: nativeEnv,
      startNative,
      startToolsCore,
    });
    expect(kind).toBe('native');
    expect(startNative).toHaveBeenCalledOnce();
    expect(startToolsCore).not.toHaveBeenCalled();
  });

  it('falls back to tools-core and warns when native start throws', async () => {
    const startNative = vi.fn(async () => {
      throw new Error('addon boom');
    });
    const startToolsCore = vi.fn(async () => {});
    const warn = vi.fn();
    const kind = await bootRuntime({
      env: nativeEnv,
      startNative,
      startToolsCore,
      warn,
    });
    expect(kind).toBe('tools-core');
    expect(startNative).toHaveBeenCalledOnce();
    expect(startToolsCore).toHaveBeenCalledOnce();
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining('falling back to tools-core: addon boom')
    );
  });
});
