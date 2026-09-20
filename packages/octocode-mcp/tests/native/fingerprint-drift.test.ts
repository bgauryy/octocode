import { afterEach, describe, expect, it, vi } from 'vitest';
import { getNativeContractFingerprint } from '@octocodeai/octocode-core/schema';
import { createNativeMcp } from '../../src/native/index.mjs';

class FakeRuntime {
  readonly abiVersion = 2;
  closed = false;

  constructor(private readonly fingerprint: string) {}

  catalog() {
    return {
      fingerprint: this.fingerprint,
      mcpInstructions: 'test instructions',
      tools: [
        {
          name: 'localFetch',
          available: true,
          inputSchema: { type: 'object' },
          outputSchema: { type: 'object' },
        },
      ],
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

const binding = (fingerprint: string) => ({
  NativeRuntime: class extends FakeRuntime {
    constructor() {
      super(fingerprint);
    }
  },
});

describe('native/core contract identity', () => {
  afterEach(() => vi.restoreAllMocks());

  it('is quiet when native and core were built from the same contract', async () => {
    const stderr = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const instance = createNativeMcp({
      binding: binding(getNativeContractFingerprint()),
      env: {},
    });

    expect(stderr).not.toHaveBeenCalled();
    await instance.close();
  });

  it('reports both fingerprints when the embedded contracts differ', async () => {
    const stderr = vi.spyOn(console, 'error').mockImplementation(() => undefined);
    const instance = createNativeMcp({
      binding: binding('0'.repeat(64)),
      env: {},
    });

    expect(stderr).toHaveBeenCalledOnce();
    expect(stderr.mock.calls[0]?.[0]).toContain('0'.repeat(64));
    expect(stderr.mock.calls[0]?.[0]).toContain(getNativeContractFingerprint());
    await instance.close();
  });
});
