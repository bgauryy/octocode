import { describe, expect, it } from 'vitest';
import { buildShorthandInput } from '../../src/oql/shorthand.js';
import { normalizeQuery } from '../../src/oql/normalize.js';

function input(r: ReturnType<typeof buildShorthandInput>) {
  if ('error' in r) throw new Error(r.error);
  return r.input;
}

describe('buildShorthandInput (CLI shorthand lowering, owned by tools-core)', () => {
  it('text + local corpus -> text sugar that normalizes to code/text', () => {
    const query = input(
      buildShorthandInput({
        text: 'runCLI',
        corpus: { kind: 'local', path: './src' },
      })
    );
    expect(query).toMatchObject({
      schema: 'oql',
      target: 'code',
      from: { kind: 'local', path: './src' },
      where: { kind: 'text', value: 'runCLI' },
    });
    const n = normalizeQuery(query as never);
    expect(n.target).toBe('code');
    expect(n.where).toEqual({ kind: 'text', value: 'runCLI' });
  });

  it('pattern requires lang', () => {
    const r = buildShorthandInput({
      pattern: 'eval($X)',
      corpus: { kind: 'local', path: '.' },
    });
    expect('error' in r && r.error).toMatch(/--pattern requires --lang/);
  });

  it('regex + pcre2 -> canonical regex where with dialect', () => {
    const query = input(
      buildShorthandInput({
        regex: 'a(?=b)',
        pcre2: true,
        corpus: { kind: 'local', path: '.' },
      })
    );
    expect((query as { where?: unknown }).where).toMatchObject({
      kind: 'regex',
      value: 'a(?=b)',
      dialect: 'pcre2',
    });
  });

  it('package shorthand keeps the positional in params instead of where', () => {
    const query = input(
      buildShorthandInput({
        target: 'packages',
        text: 'zod',
        corpus: { kind: 'npm' },
      })
    );
    expect(query).toMatchObject({
      target: 'packages',
      from: { kind: 'npm' },
      params: { packageName: 'zod' },
    });
    expect((query as { where?: unknown }).where).toBeUndefined();
  });

  it('github corpus + lang -> repo/path/ref + language scope', () => {
    const query = input(
      buildShorthandInput({
        text: 'useEffect',
        lang: 'tsx',
        corpus: {
          kind: 'github',
          repo: 'facebook/react',
          path: 'packages/react',
          ref: 'main',
        },
      })
    );
    expect(query).toMatchObject({
      target: 'code',
      from: { kind: 'github', repo: 'facebook/react', ref: 'main' },
      scope: { path: 'packages/react', language: 'tsx' },
      where: { kind: 'text', value: 'useEffect' },
    });
  });

  it('predicate precedence pattern > regex > text', () => {
    const query = input(
      buildShorthandInput({
        pattern: 'foo($X)',
        lang: 'ts',
        regex: 'bar',
        text: 'baz',
        corpus: { kind: 'local', path: '.' },
      })
    );
    expect(query.where).toEqual({
      kind: 'structural',
      lang: 'ts',
      pattern: 'foo($X)',
    });
  });

  it('content flags lower to target content fetch instructions', () => {
    const query = input(
      buildShorthandInput({
        target: 'content',
        contentView: 'exact',
        startLine: 10,
        endLine: 20,
        contextLines: 2,
        corpus: { kind: 'local', path: './src/index.ts' },
      })
    );
    expect(query).toMatchObject({
      target: 'content',
      from: { kind: 'local', path: './src/index.ts' },
      fetch: {
        content: {
          contentView: 'exact',
          range: { startLine: 10, endLine: 20, contextLines: 2 },
        },
      },
    });
  });

  it('LSP operation lowers to semantics params', () => {
    const query = input(
      buildShorthandInput({
        op: 'references',
        symbol: 'runCLI',
        symbolKind: 'function',
        line: 42,
        corpus: { kind: 'local', path: './src/index.ts' },
      })
    );
    expect(query).toMatchObject({
      target: 'semantics',
      from: { kind: 'local', path: './src/index.ts' },
      params: {
        type: 'references',
        symbolName: 'runCLI',
        symbolKind: 'function',
        lineHint: 42,
      },
    });
  });

  it('repository sort lowers to target params, not local search controls', () => {
    const query = input(
      buildShorthandInput({
        target: 'repositories',
        text: 'mcp server',
        lang: 'TypeScript',
        sort: 'stars',
        corpus: { kind: 'github', repo: '' },
      })
    );
    expect(query).toMatchObject({
      target: 'repositories',
      from: { kind: 'github' },
      params: {
        keywords: ['mcp server'],
        language: 'TypeScript',
        sort: 'stars',
      },
    });
    expect(query.controls).toBeUndefined();
  });

  it('search controls lower into canonical controls', () => {
    const query = input(
      buildShorthandInput({
        text: 'TODO',
        contextLines: 1,
        invertMatch: true,
        onlyMatching: true,
        maxMatchesPerFile: 5,
        corpus: { kind: 'local', path: '.' },
      })
    );
    expect(query).toMatchObject({
      controls: {
        search: {
          contextLines: 1,
          invertMatch: true,
          onlyMatching: true,
          maxMatchesPerFile: 5,
        },
      },
    });
  });

  it('path search builds a file-field predicate', () => {
    const query = input(
      buildShorthandInput({
        target: 'files',
        search: 'path',
        text: 'index.ts',
        extension: 'ts',
        corpus: { kind: 'local', path: './src' },
      })
    );
    expect(query.target).toBe('files');
    expect(query.where).toMatchObject({
      kind: 'all',
      of: [
        { kind: 'field', field: 'extension', op: '=', value: 'ts' },
        { kind: 'field', field: 'basename', op: 'glob', value: '*index.ts*' },
      ],
    });
  });

  it('path regex search lowers to a files path-regex predicate', () => {
    const query = input(
      buildShorthandInput({
        target: 'files',
        search: 'path',
        regex: '.*\\.test\\.ts$',
        corpus: { kind: 'local', path: './src' },
      })
    );
    expect(query).toMatchObject({
      target: 'files',
      from: { kind: 'local', path: './src' },
      where: {
        kind: 'field',
        field: 'path',
        op: 'regex',
        value: '.*\\.test\\.ts$',
      },
    });
  });

  it('structure shorthand maps ls filters to tree fetch, not search controls', () => {
    const query = input(
      buildShorthandInput({
        target: 'structure',
        pattern: 's*.ts',
        extension: 'ts,tsx',
        filesOnly: true,
        sort: 'name',
        sortReverse: true,
        limit: 5,
        corpus: { kind: 'local', path: './src' },
      })
    );
    expect(query).toMatchObject({
      target: 'structure',
      from: { kind: 'local', path: './src' },
      fetch: {
        tree: {
          pattern: 's*.ts',
          extensions: ['ts', 'tsx'],
          filesOnly: true,
          sortBy: 'name',
          reverse: true,
        },
      },
      limit: 5,
    });
    expect((query as { where?: unknown }).where).toBeUndefined();
    expect((query as { controls?: unknown }).controls).toBeUndefined();
  });

  it('PR patch-file shorthand lowers to a selected patch request', () => {
    const query = input(
      buildShorthandInput({
        target: 'pullRequests',
        prNumber: 123,
        patchFile: 'src/index.ts',
        corpus: { kind: 'github', repo: 'facebook/react' },
      })
    );
    expect(query).toMatchObject({
      target: 'pullRequests',
      from: { kind: 'github', repo: 'facebook/react' },
      params: {
        prNumber: 123,
        content: {
          metadata: true,
          changedFiles: true,
          patches: { mode: 'selected', files: ['src/index.ts'] },
        },
      },
    });
  });
});
