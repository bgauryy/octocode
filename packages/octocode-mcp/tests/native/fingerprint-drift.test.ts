import { afterEach, describe, expect, it, vi } from 'vitest';
import { getNativeContractFingerprint } from '@octocodeai/config/schema';
import { createNativeMcp } from '../../src/native/index.js';
import { NATIVE_ABI_VERSION } from '@octocodeai/octocode-native/runtime';

class FakeRuntime {
  readonly abiVersion = NATIVE_ABI_VERSION;
  closed = false;

  constructor(private readonly fingerprint: string) {}

  async probeClassification() {
    return { probed: false, available: false };
  }

  catalog() {
    return {
      fingerprint: this.fingerprint,
      tools: [{ name: 'localFetch', available: true }],
    };
  }

  cancel() {
    return true;
  }

  async executeMcp() {
    return { content: [], isError: false };
  }

  async close() {
    this.closed = true;
  }
}

const created: FakeRuntime[] = [];
const binding = (fingerprint: string) => ({
  NativeRuntime: class extends FakeRuntime {
    constructor() {
      super(fingerprint);
      created.push(this);
    }
  },
});

describe('native/core contract identity', () => {
  afterEach(() => {
    vi.restoreAllMocks();
    created.length = 0;
  });

  it('is quiet when native and core were built from the same contract', async () => {
    const stderr = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true);
    const instance = await createNativeMcp({
      binding: binding(getNativeContractFingerprint()),
      env: {},
    });

    expect(stderr).not.toHaveBeenCalled();
    await instance.close();
  });

  it('fails closed when the embedded contracts differ', async () => {
    const stderr = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true);
    // A rejected start must surface both fingerprints and leak no live runtime.
    const error = await createNativeMcp({
      binding: binding('0'.repeat(64)),
      env: {},
    }).then(
      () => undefined,
      (thrown: unknown) => thrown as Error
    );
    expect(error?.message).toMatch(/fingerprint mismatch/);
    expect(error?.message).toContain('0'.repeat(64));
    expect(error?.message).toContain(getNativeContractFingerprint());
    // The override hint names every condition it needs: the bundled dist
    // honors it only under NODE_ENV=development (never production).
    expect(error?.message).toContain('OCTOCODE_ALLOW_CONTRACT_DRIFT=1');
    expect(error?.message).toContain('NODE_ENV=development');
    expect(created.every(runtime => runtime.closed)).toBe(true);
    expect(stderr).not.toHaveBeenCalled();
  });

  it('downgrades to a warning when the drift override is set', async () => {
    const stderr = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true);
    const instance = await createNativeMcp({
      binding: binding('0'.repeat(64)),
      env: { OCTOCODE_ALLOW_CONTRACT_DRIFT: '1' },
    });

    expect(stderr).toHaveBeenCalledOnce();
    expect(stderr.mock.calls[0]?.[0]).toContain('0'.repeat(64));
    expect(stderr.mock.calls[0]?.[0]).toContain(getNativeContractFingerprint());
    await instance.close();
  });
});
