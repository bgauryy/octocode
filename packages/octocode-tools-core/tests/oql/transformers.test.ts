import { describe, expect, it } from 'vitest';
import { normalizeQuery } from '../../src/oql/normalize.js';
import { toGithubCodeSearchToolQuery } from '../../src/oql/transformers/github/code.js';
import { leafPredicates } from '../../src/oql/transformers/github/common.js';
import {
  classifyLanguageSelector,
  toGithubCodeLanguageParams,
  toGithubRepositoryLanguage,
} from '../../src/oql/transformers/language.js';

function githubCodeQuery(input: unknown) {
  return normalizeQuery(input as never);
}

describe('OQL transformers: language selectors', () => {
  it('treats short file-like selectors as extensions for GitHub code search', () => {
    expect(classifyLanguageSelector('ts')).toMatchObject({
      kind: 'extension',
      canonicalLanguage: 'TypeScript',
      extension: 'ts',
    });
    expect(toGithubCodeLanguageParams('ts')).toEqual({ extension: 'ts' });
    expect(toGithubCodeLanguageParams('.tsx')).toEqual({ extension: 'tsx' });
  });

  it('treats language names as canonical provider languages', () => {
    expect(classifyLanguageSelector('typescript')).toMatchObject({
      kind: 'language',
      canonicalLanguage: 'TypeScript',
      extensions: ['ts', 'tsx', 'mts', 'cts'],
    });
    expect(toGithubCodeLanguageParams('typescript')).toEqual({
      language: 'TypeScript',
    });
    expect(toGithubRepositoryLanguage('ts')).toBe('TypeScript');
  });
});

describe('OQL transformers: GitHub code search query', () => {
  it('lowers OQL shorthand language ts to ghSearchCode extension ts', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', repo: 'vercel/next.js' },
        scope: { language: 'ts' },
        where: { kind: 'text', value: 'NextRequest' },
        limit: 3,
      })
    );

    expect(transformed.ok).toBe(true);
    if (!transformed.ok) throw new Error('expected transform to succeed');
    expect(transformed.query).toMatchObject({
      owner: 'vercel',
      repo: 'next.js',
      keywords: ['NextRequest'],
      extension: 'ts',
      limit: 3,
    });
    expect(transformed.query).not.toHaveProperty('language');
  });

  it('rejects an empty search term instead of emitting keywords:[""] (H2)', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', repo: 'facebook/react' },
        where: { kind: 'text', value: '' },
      })
    );

    expect(transformed.ok).toBe(false);
    if (transformed.ok) throw new Error('expected transform to fail');
    expect(transformed.diagnostics[0]).toMatchObject({
      code: 'vendorNoEquivalent',
    });
  });

  it('passes language-name selectors to the GitHub language qualifier', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', owner: 'vuejs', repo: 'core' },
        scope: { language: 'TypeScript', path: 'packages/runtime-core/src' },
        where: { kind: 'text', value: 'createApp' },
      })
    );

    expect(transformed.ok).toBe(true);
    if (!transformed.ok) throw new Error('expected transform to succeed');
    expect(transformed.query).toMatchObject({
      owner: 'vuejs',
      repo: 'core',
      keywords: ['createApp'],
      language: 'TypeScript',
      path: 'packages/runtime-core/src',
    });
    expect(transformed.query).not.toHaveProperty('extension');
  });

  it('lets explicit params.extension override scope.language', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', repo: 'langchain-ai/langchainjs' },
        scope: { language: 'typescript' },
        where: { kind: 'text', value: 'BaseChatModel' },
        params: { extension: 'ts', match: 'file', concise: true },
      })
    );

    expect(transformed.ok).toBe(true);
    if (!transformed.ok) throw new Error('expected transform to succeed');
    expect(transformed.query).toMatchObject({
      owner: 'langchain-ai',
      repo: 'langchainjs',
      extension: 'ts',
      match: 'file',
      concise: true,
    });
    expect(transformed.query).not.toHaveProperty('language');
  });

  it('defaults GitHub files target searches to file-content matching', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'files',
        from: { kind: 'github', repo: 'pmndrs/zustand' },
        scope: { language: 'ts' },
        where: { kind: 'text', value: 'vanilla' },
      }),
      { defaultMatch: 'file' }
    );

    expect(transformed.ok).toBe(true);
    if (!transformed.ok) throw new Error('expected transform to succeed');
    expect(transformed.query).toMatchObject({
      owner: 'pmndrs',
      repo: 'zustand',
      keywords: ['vanilla'],
      extension: 'ts',
      match: 'file',
    });
  });

  it('blocks structural predicates instead of inventing a GitHub query', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', repo: 'facebook/react' },
        where: { kind: 'structural', lang: 'js', pattern: 'useState($$$ARGS)' },
      })
    );

    expect(transformed.ok).toBe(false);
    if (transformed.ok) throw new Error('expected transform to fail');
    expect(transformed.diagnostics[0]?.code).toBe('unsupportedVendorPredicate');
    expect(transformed.diagnostics[0]?.blocksAnswer).toBe(true);
  });

  it('splits a multi-word text term into separate AND keywords for GitHub (CLI parity with MCP)', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', repo: 'vercel/next.js' },
        where: { kind: 'text', value: 'output export static' },
      })
    );
    expect(transformed.ok).toBe(true);
    if (!transformed.ok) throw new Error('expected transform to succeed');
    expect(transformed.query.keywords).toEqual(['output', 'export', 'static']);
  });

  it('does not split a regex term even when it contains spaces', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', repo: 'facebook/react' },
        where: { kind: 'regex', value: 'foo bar' },
      })
    );
    expect(transformed.ok).toBe(true);
    if (!transformed.ok) throw new Error('expected transform to succeed');
    expect(transformed.query.keywords).toEqual(['foo bar']);
  });

  it('blocks lossy multi-scope mappings instead of silently dropping values', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', repo: 'facebook/react' },
        scope: { language: ['ts', 'tsx'] },
        where: { kind: 'text', value: 'useState' },
      })
    );

    expect(transformed.ok).toBe(false);
    if (transformed.ok) throw new Error('expected transform to fail');
    expect(transformed.diagnostics[0]).toMatchObject({
      code: 'lossyTransform',
      queryPath: 'scope.language',
      blocksAnswer: true,
    });
  });
});

describe('OQL transformers: predicate consumption fails closed', () => {
  it('rejects a multi-leaf boolean instead of dropping a branch', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', repo: 'facebook/react' },
        where: {
          kind: 'all',
          of: [
            { kind: 'text', value: 'useEffect' },
            { kind: 'text', value: 'useState' },
          ],
        },
        materialize: 'never',
      })
    );
    expect(transformed.ok).toBe(false);
    if (transformed.ok) throw new Error('expected transform to fail');
    expect(transformed.diagnostics[0]?.blocksAnswer).toBe(true);
  });

  it('rejects not(text) — GitHub cannot prove absence', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'code',
        from: { kind: 'github', repo: 'facebook/react' },
        where: {
          kind: 'not',
          predicate: { kind: 'text', value: 'useEffect' },
        },
        materialize: 'never',
      })
    );
    expect(transformed.ok).toBe(false);
    if (transformed.ok) throw new Error('expected transform to fail');
    expect(transformed.diagnostics[0]?.code).toBe('unsupportedVendorPredicate');
  });

  it('rejects a non-equality field predicate (no provider qualifier)', () => {
    const transformed = toGithubCodeSearchToolQuery(
      githubCodeQuery({
        target: 'files',
        from: { kind: 'github', repo: 'facebook/react' },
        where: { kind: 'field', field: 'size', op: '>', value: 1024 },
        materialize: 'never',
      })
    );
    expect(transformed.ok).toBe(false);
    if (transformed.ok) throw new Error('expected transform to fail');
    expect(transformed.diagnostics[0]?.blocksAnswer).toBe(true);
  });

  it('leafPredicates collects every leaf through boolean structure', () => {
    const leaves = leafPredicates({
      kind: 'any',
      of: [
        { kind: 'text', value: 'a' },
        {
          kind: 'all',
          of: [
            { kind: 'regex', value: 'b' },
            { kind: 'not', predicate: { kind: 'text', value: 'c' } },
          ],
        },
      ],
    });
    expect(leaves.map(l => l.kind)).toEqual(['text', 'regex', 'text']);
  });
});
