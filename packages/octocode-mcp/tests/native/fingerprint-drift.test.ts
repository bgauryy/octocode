import { afterEach, describe, expect, it, vi } from 'vitest';
import { getNativeContractFingerprint } from '@octocodeai/octocode-core/schema';
import { createNativeMcp } from '../../src/native/index.js';

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
    const instance = createNativeMcp({
      binding: binding(getNativeContractFingerprint()),
      env: {},
    });

    expect(stderr).not.toHaveBeenCalled();
    await instance.close();
  });

  it('fails closed when the embedded contracts differ', () => {
    const stderr = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true);
    expect(() =>
      createNativeMcp({ binding: binding('0'.repeat(64)), env: {} })
    ).toThrow(/fingerprint mismatch/);
    // A rejected start must surface both fingerprints and leak no live runtime.
    const error = (() => {
      try {
        createNativeMcp({ binding: binding('0'.repeat(64)), env: {} });
        return undefined;
      } catch (thrown) {
        return thrown as Error;
      }
    })();
    expect(error?.message).toContain('0'.repeat(64));
    expect(error?.message).toContain(getNativeContractFingerprint());
    expect(created.every(runtime => runtime.closed)).toBe(true);
    expect(stderr).not.toHaveBeenCalled();
  });

  it('downgrades to a warning when the drift override is set', async () => {
    const stderr = vi
      .spyOn(process.stderr, 'write')
      .mockImplementation(() => true);
    const instance = createNativeMcp({
      binding: binding('0'.repeat(64)),
      env: { OCTOCODE_ALLOW_CONTRACT_DRIFT: '1' },
    });

    expect(stderr).toHaveBeenCalledOnce();
    expect(stderr.mock.calls[0]?.[0]).toContain('0'.repeat(64));
    expect(stderr.mock.calls[0]?.[0]).toContain(getNativeContractFingerprint());
    await instance.close();
  });
});
