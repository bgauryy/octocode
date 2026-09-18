import { afterEach, describe, expect, it } from 'vitest';
import { buildRuleConfigJson } from '../../../src/tools/ast_rewrite/nativeRewrite.js';
import { evaluatePostconditions } from '../../../src/tools/ast_rewrite/postcondition.js';
import type {
  AstRewriteQuery,
  PreparedFile,
} from '../../../src/tools/ast_rewrite/types.js';
import {
  resetContextUtilsNativeLoaderForTesting,
  setContextUtilsNativeLoaderForTesting,
} from '../../../src/utils/contextUtils.js';

afterEach(() => resetContextUtilsNativeLoaderForTesting());

function baseQuery(
  overrides: Partial<AstRewriteQuery> = {}
): AstRewriteQuery {
  return {
    path: '/workspace',
    langType: 'typescript',
    ruleKind: 'pattern',
    pattern: 'oldCall($A)',
    rewrite: 'newCall($A)',
    ...overrides,
  } as AstRewriteQuery;
}

function preparedFile(after = 'newCall(1);'): PreparedFile {
  const bytes = Buffer.from(after);
  return {
    path: 'source.ts',
    absolutePath: '/workspace/source.ts',
    beforeHash: 'before',
    afterHash: 'after',
    matchCount: 1,
    patch: '',
    patchBytes: 0,
    before: Buffer.from('oldCall(1);'),
    after: bytes,
    mode: 0o644,
    matches: [],
  };
}

describe('native astRewrite rule configuration', () => {
  it('expands pattern shorthand into one embedded inline rule', () => {
    expect(JSON.parse(buildRuleConfigJson(baseQuery()))).toMatchObject({
      language: 'typescript',
      rule: { pattern: 'oldCall($A)' },
      fix: 'newCall($A)',
    });
  });

  it('forwards every optional inline-rule field including utils', () => {
    const config = JSON.parse(
      buildRuleConfigJson(
        baseQuery({
          ruleKind: 'rule',
          pattern: undefined,
          rewrite: undefined,
          rule: { kind: 'call_expression' },
          fix: 'replacement',
          constraints: { A: { regex: '^value$' } },
          utils: { named: { kind: 'identifier' } },
          transform: { OUT: { substring: { source: '$A', startChar: 1 } } },
          rewriters: [{ id: 'ignored-for-rule' }],
        })
      )
    );
    expect(config).toMatchObject({
      rule: { kind: 'call_expression' },
      fix: 'replacement',
      constraints: { A: { regex: '^value$' } },
      utils: { named: { kind: 'identifier' } },
      transform: { OUT: { substring: { source: '$A', startChar: 1 } } },
    });
    expect(config).not.toHaveProperty('rewriters');
  });

  it('includes rewriters only for experimental rules and omits absent fields', () => {
    const config = JSON.parse(
      buildRuleConfigJson(
        baseQuery({
          ruleKind: 'experimental',
          pattern: undefined,
          rewrite: undefined,
          rule: { pattern: 'oldCall($A)' },
          fix: 'newCall($A)',
          rewriters: [{ id: 'rename', rule: { kind: 'identifier' }, fix: 'x' }],
        })
      )
    );
    expect(config.rewriters).toHaveLength(1);
    expect(config).not.toHaveProperty('constraints');
    expect(config).not.toHaveProperty('utils');
    expect(config).not.toHaveProperty('transform');
  });
});

describe('native astRewrite postconditions', () => {
  it('uses the stable fallback when the engine throws a non-Error value', async () => {
    setContextUtilsNativeLoaderForTesting(
      () =>
        ({
          structuralRewriteContent: () => {
            throw 'native failure';
          },
        }) as never
    );
    const result = await evaluatePostconditions(
      baseQuery({ postconditions: [{ kind: 'remainingMatches', equals: 0 }] }),
      'native',
      [preparedFile()],
      {}
    );
    expect(result).toMatchObject({
      ok: false,
      result: {
        errorCode: 'ast.rewrite.postcondition_failed',
        error: 'Native postcondition evaluation failed.',
      },
    });
  });
});
