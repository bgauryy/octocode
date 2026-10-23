import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { getNativeContractFingerprint } from '@octocodeai/config/schema';
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';
import { createNativeMcp, loadNativeBinding } from '../../src/native/index.js';
import { bindingFor, FakeRuntime, tool } from './native-mcp-fixture.js';

const fixture = (name: string) =>
  fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url));

afterEach(() => {
  FakeRuntime.last = undefined;
});

describe('createNativeMcp fails closed', () => {
  it('throws and closes the runtime when a catalog tool has no core contract', async () => {
    await expect(
      createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [tool('not-a-real-octocode-tool', true)],
        })),
      })
    ).rejects.toThrow(/no contract/i);
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('throws and closes the runtime when no tools are available', async () => {
    await expect(
      createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: getNativeContractFingerprint(),
          tools: [tool('localFetch', false)],
        })),
      })
    ).rejects.toThrow(/No native tools are available/i);
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('throws and closes the runtime on an ABI mismatch', async () => {
    const binding = {
      NativeRuntime: class extends FakeRuntime {
        readonly abiVersion = 999999;
        constructor() {
          super(() => ({
            fingerprint: getNativeContractFingerprint(),
            tools: [tool('localFetch', true)],
          }));
        }
      },
    };
    await expect(createNativeMcp({ env: {}, binding })).rejects.toThrow(/ABI/);
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('fails closed on the previous ABI (an addon without per-request options)', async () => {
    const binding = {
      NativeRuntime: class extends FakeRuntime {
        readonly abiVersion = NATIVE_ABI_VERSION - 1;
        constructor() {
          super(() => ({
            fingerprint: getNativeContractFingerprint(),
            tools: [tool('localFetch', true)],
          }));
        }
      },
    };
    await expect(createNativeMcp({ env: {}, binding })).rejects.toThrow(
      `Native addon ABI ${NATIVE_ABI_VERSION - 1} does not match expected ${NATIVE_ABI_VERSION}`
    );
    expect(FakeRuntime.last!.closed).toBe(true);
  });

  it('throws and closes the runtime when the catalog exposes no fingerprint', async () => {
    await expect(
      createNativeMcp({
        env: {},
        binding: bindingFor(() => ({
          fingerprint: '',
          tools: [tool('localFetch', true)],
        })),
      })
    ).rejects.toThrow(/does not expose a contract fingerprint/i);
    expect(FakeRuntime.last!.closed).toBe(true);
  });
});

describe('loadNativeBinding', () => {
  it('resolves a candidate addon that exports NativeRuntime', () => {
    const binding = loadNativeBinding({
      OCTOCODE_NATIVE_BINDING: fixture('fake-addon.cjs'),
    });
    expect(typeof binding.NativeRuntime).toBe('function');
  });

  it('throws when the candidate addon does not export NativeRuntime', () => {
    expect(() =>
      loadNativeBinding({
        OCTOCODE_NATIVE_BINDING: fixture('addon-without-runtime.cjs'),
      })
    ).toThrow(/does not export NativeRuntime/i);
  });
});
