import { beforeEach, describe, expect, it, vi } from 'vitest';

const native = vi.hoisted(() => ({
  loadNativeBinding: vi.fn(),
  createNativeMcp: vi.fn(),
  startNativeMcp: vi.fn(),
}));

vi.mock('../src/native/index.mjs', () => native);

import {
  createNativeMcp,
  loadNativeBinding,
  startNativeMcp,
} from '../src/public.js';

describe('public native adapter', () => {
  beforeEach(() => vi.clearAllMocks());

  it('forwards binding resolution options without adding policy', () => {
    const binding = { NativeRuntime: class {} };
    native.loadNativeBinding.mockReturnValue(binding);
    const env = { OCTOCODE_NATIVE_BINDING: '/tmp/native.node' };

    expect(loadNativeBinding(env)).toBe(binding);
    expect(native.loadNativeBinding).toHaveBeenCalledWith(env);
  });

  it('forwards server construction options', () => {
    const instance = { close: vi.fn() };
    const options = { env: { TOOLS_TO_RUN: 'localFetch' } };
    native.createNativeMcp.mockReturnValue(instance);

    expect(createNativeMcp(options)).toBe(instance);
    expect(native.createNativeMcp).toHaveBeenCalledWith(options);
  });

  it('forwards native startup and preserves its result', async () => {
    const instance = { close: vi.fn() };
    native.startNativeMcp.mockResolvedValue(instance);

    await expect(startNativeMcp()).resolves.toBe(instance);
    expect(native.startNativeMcp).toHaveBeenCalledWith(undefined);
  });
});
