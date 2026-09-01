import { afterEach, describe, expect, it } from 'vitest';

import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';
import { createResponseFormat } from '../../../src/responses.js';
import { buildToolResultMeta } from '../../../src/utils/response/bulk/response.js';

type NativeContextUtilsModule = typeof import('@octocodeai/octocode-engine');

function installNative(partial: Partial<NativeContextUtilsModule>): void {
  setContextUtilsNativeLoaderForTesting(
    () => partial as NativeContextUtilsModule
  );
}

describe('response YAML formatter contract', () => {
  afterEach(() => {
    resetContextUtilsNativeLoaderForTesting();
  });

  it('accepts a nested continueChars call as the executable continuation for a partial remote file', () => {
    const meta = buildToolResultMeta(
      'ghGetFileContent',
      {},
      {
        files: [
          {
            isPartial: true,
            pagination: { hasMore: true },
            next: {
              continueChars: {
                tool: 'ghGetFileContent',
                query: { owner: 'o', repo: 'r', path: 'p', charOffset: 10 },
              },
            },
          },
        ],
      }
    );

    expect(meta.diagnostics?.partial).toBe(true);
    expect(meta.diagnostics?.codes ?? []).not.toContain('continuationMissing');
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

  it('prioritizes the canonical ordered bulk row fields', () => {
    const calls: NonNullable<
      Parameters<NativeContextUtilsModule['jsonToYamlString']>[1]
    >[] = [];
    installNative({
      jsonToYamlString: (_jsonObject, config) => {
        calls.push(config ?? {});
        return 'results: []\n';
      },
    });

    expect(createResponseFormat({ results: [] })).toBe('results: []\n');
    expect(calls).toEqual([
      {
        keysPriority: ['results', 'index', 'status', 'cache', 'meta', 'data'],
      },
    ]);
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

  it('normalizes evidence and diagnostics metadata across tool families', () => {
    expect(
      buildToolResultMeta(
        'localAnalyzeGraph',
        { operation: 'dependencies' },
        { pagination: { hasMore: true } }
      )
    ).toEqual({
      evidence: { kind: 'syntactic', confidence: 'medium' },
      diagnostics: { partial: true, codes: ['continuationMissing'] },
    });
    expect(
      buildToolResultMeta(
        'localAnalyzeGraph',
        { operation: 'dependencies' },
        {
          pagination: { hasMore: true },
          next: {
            nextPage: {
              tool: 'localAnalyzeGraph',
              query: { operation: 'dependencies', page: 2 },
            },
          },
        }
      )
    ).toEqual({
      evidence: { kind: 'syntactic', confidence: 'medium' },
      diagnostics: { partial: true },
    });
    expect(
      buildToolResultMeta(
        'ghGetFileContent',
        {},
        {
          files: [
            {
              isPartial: true,
              next: {
                continueChars: {
                  tool: 'ghGetFileContent',
                  query: { owner: 'o', repo: 'r', path: 'p', charOffset: 200 },
                },
              },
            },
          ],
        }
      )
    ).toEqual({
      evidence: { kind: 'provider', confidence: 'medium' },
      diagnostics: { partial: true },
    });
    expect(
      buildToolResultMeta(
        'ghGetHistoryItem',
        {},
        {
          contentPagination: { body: { hasMore: true } },
        }
      )
    ).toEqual({
      evidence: { kind: 'provider', confidence: 'medium' },
      diagnostics: { partial: true, codes: ['continuationMissing'] },
    });
    expect(
      buildToolResultMeta(
        'ghGetFileContent',
        {},
        {
          complete: false,
          next: {
            viewStructure: {
              tool: 'localSearch',
              query: { operation: 'tree', path: '/tmp/repo' },
            },
          },
        }
      )
    ).toEqual({
      evidence: { kind: 'provider', confidence: 'medium' },
      diagnostics: { partial: true, codes: ['continuationMissing'] },
    });
    expect(
      buildToolResultMeta(
        'ghSearch',
        {},
        {
          pagination: { hasMore: true },
          next: {
            fetchFile: {
              tool: 'ghGetFileContent',
              query: { owner: 'o', repo: 'r', path: 'p' },
            },
          },
        }
      )
    ).toEqual({
      evidence: { kind: 'provider', confidence: 'medium' },
      diagnostics: { partial: true, codes: ['continuationMissing'] },
    });
    expect(
      buildToolResultMeta(
        'localAnalyzeGraph',
        { operation: 'cycles' },
        { truncated: true, terminalLimit: true }
      )
    ).toEqual({
      evidence: { kind: 'syntactic', confidence: 'medium' },
      diagnostics: { partial: true, codes: ['terminalLimitReached'] },
    });
    expect(
      buildToolResultMeta(
        'ghGetFileContent',
        {},
        {
          directories: [
            {
              complete: false,
              isPartial: true,
              next: {
                escalateToClone: {
                  tool: 'ghCloneRepo',
                  query: { owner: 'o', repo: 'r', sparsePath: 'src' },
                },
              },
            },
          ],
        }
      )
    ).toEqual({
      evidence: { kind: 'provider', confidence: 'medium' },
      diagnostics: { partial: true },
    });
    expect(
      buildToolResultMeta(
        'ghGetFileContent',
        {},
        {
          directories: [
            {
              complete: false,
              isPartial: true,
              terminalLimit: true,
            },
          ],
        }
      )
    ).toEqual({
      evidence: { kind: 'provider', confidence: 'medium' },
      diagnostics: { partial: true, codes: ['terminalLimitReached'] },
    });
    expect(
      buildToolResultMeta('lspGetSemantics', { type: 'references' }, {})
    ).toEqual({ evidence: { kind: 'semantic', confidence: 'high' } });
    expect(
      buildToolResultMeta(
        'github.code',
        {},
        { errorCode: 'rateLimited', hints: ['retry later'] },
        'error'
      )
    ).toEqual({
      evidence: { kind: 'provider', confidence: 'low' },
      diagnostics: { codes: ['rateLimited'] },
    });
  });
});
