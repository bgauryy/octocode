import { afterEach, describe, expect, it } from 'vitest';

import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';
import { createResponseFormat } from '../../../src/responses.js';

type NativeContextUtilsModule = typeof import('@octocodeai/octocode-context-utils');

function installNative(partial: Partial<NativeContextUtilsModule>): void {
  setContextUtilsNativeLoaderForTesting(() => partial as NativeContextUtilsModule);
}

describe('response YAML formatter contract', () => {
  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('passes priority keys to context-utils YAML serialization', () => {
    const calls: NonNullable<
      Parameters<NativeContextUtilsModule['jsonToYamlString']>[1]
    >[] = [];
    installNative({
      jsonToYamlString: (_jsonObject, config) => {
        calls.push(config ?? {});
        return 'status: ok\n';
      },
    });

    expect(
      createResponseFormat(
        { status: 'ok', instructions: 'read first', data: { b: 2, a: 1 } },
        ['data', 'status']
      )
    ).toBe('status: ok\n');
    expect(calls).toEqual([{ keysPriority: ['data', 'status'] }]);
  });
});
