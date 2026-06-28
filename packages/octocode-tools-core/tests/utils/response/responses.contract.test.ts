import { afterEach, describe, expect, it } from 'vitest';

import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';
import { createResponseFormat } from '../../../src/responses.js';

type NativeContextUtilsModule = typeof import('@octocodeai/octocode-engine');

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

  it('redacts secrets in the formatted text output without leaking the raw value', () => {
    // Real serializer + real sanitizer (no installNative) — proves the
    // per-field sanitization still redacts in the rendered text after dropping
    // the whole-document scan.
    const PAT = 'ghp_1234567890abcdefghijklmnopqrstuvwxyzAB';
    const out = createResponseFormat({
      status: 'ok',
      data: {
        files: [
          { path: 'a.ts', snippet: `const t = "${PAT}";` },
          { path: 'b.ts', snippet: 'foo(bar)' },
        ],
      },
    });
    expect(out).not.toContain(PAT);
    expect(out).toContain('[REDACTED-');
    expect(out).toContain('foo(bar)'); // benign content preserved
  });
});
